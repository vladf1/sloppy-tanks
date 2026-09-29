//! Textures: `public/` files fetched and decoded by the browser
//! (`createImageBitmap`) and copied with `copyExternalImageToTexture`, plus pixels
//! generated at runtime. Uploads flip Y like Three's `flipY` (unless the reference
//! clears `flip_y`), so UV (0,0) is the image's bottom-left; mip levels are
//! rendered from the level above.
//! Until a texture arrives, materials sample a white placeholder.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use sloppy_core::scene::{TextureRef, TextureSource, Wrap};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::gpu::precompile::{Background, LayoutKind, PipelineSpec};
use crate::shader::MIPMAP_WGSL;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextureKey {
    pub source: TextureSource,
    pub srgb: bool,
    pub mipmaps: bool,
    pub flip_y: bool,
}

impl TextureKey {
    pub fn of(texture: &TextureRef) -> Self {
        Self {
            source: texture.source.clone(),
            srgb: texture.srgb,
            mipmaps: texture.mipmaps,
            flip_y: texture.flip_y,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SamplerKey {
    wrap: Wrap,
    anisotropy: u16,
    mipmaps: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureState {
    Loading,
    Ready,
    Failed,
}

struct TextureEntry {
    texture: Option<wgpu::Texture>,
    view: Option<wgpu::TextureView>,
    bytes: u64,
    state: TextureState,
}

enum Pixels {
    Bitmap(web_sys::ImageBitmap),
    Raw {
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
}

struct Loaded {
    key: TextureKey,
    result: Result<Pixels, String>,
}

/// The mipmap blit's source level and sampler (shared with `precompile.rs`).
pub const MIPMAP_SOURCE_ENTRIES: &[wgpu::BindGroupLayoutEntry] = &[
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    },
    wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    },
];

struct MipmapGenerator {
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    /// The sRGB and linear blit specs while they compile in the background.
    pending: Option<(PipelineSpec, PipelineSpec, Background)>,
    /// The sRGB and linear blit pipelines.
    pipelines: Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)>,
    sampler: wgpu::Sampler,
}

fn mipmap_spec(format: wgpu::TextureFormat) -> PipelineSpec {
    PipelineSpec {
        label: "mipmap",
        layout: LayoutKind::Mipmap,
        vertex_entry: "vs_blit",
        fragment_entry: Some("fs_blit"),
        buffers: vec![],
        targets: vec![Some(format.into())],
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
    }
}

impl MipmapGenerator {
    /// Starts compiling both blit pipelines in the background; [`Self::ready`]
    /// creates them once compiled.
    fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mipmap source"),
            entries: MIPMAP_SOURCE_ENTRIES,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mipmap"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let srgb = mipmap_spec(wgpu::TextureFormat::Rgba8UnormSrgb);
        let linear = mipmap_spec(wgpu::TextureFormat::Rgba8Unorm);
        let compile = Background::start(device, &[(&srgb, MIPMAP_WGSL), (&linear, MIPMAP_WGSL)]);
        Self {
            pending: Some((srgb, linear, compile)),
            pipelines: None,
            pipeline_layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("mipmap"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            layout,
        }
    }

    /// Whether the blit pipelines exist, creating them once compiled.
    fn ready(&mut self, device: &wgpu::Device) -> bool {
        if let Some((srgb, linear, compile)) = &self.pending
            && compile.done()
        {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("mipmap blit"),
                source: wgpu::ShaderSource::Wgsl(MIPMAP_WGSL.into()),
            });
            self.pipelines = Some((
                srgb.create(device, &self.pipeline_layout, &module),
                linear.create(device, &self.pipeline_layout, &module),
            ));
            self.pending = None;
        }
        self.pipelines.is_some()
    }

    fn generate(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        srgb: bool,
    ) {
        let (srgb_blit, linear_blit) = self.pipelines.as_ref().expect("mipmap pipelines");
        let levels = texture.mip_level_count();
        if levels < 2 {
            return;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mipmaps"),
        });
        let level_view = |level: u32| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        for level in 1..levels {
            let source = level_view(level - 1);
            let target = level_view(level);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mipmap source"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mipmap level"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(if srgb { srgb_blit } else { linear_blit });
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
    }
}

async fn fetch_bitmap(url: &str) -> Result<web_sys::ImageBitmap, JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let response: web_sys::Response = JsFuture::from(window.fetch_with_str(url))
        .await?
        .dyn_into()?;
    if !response.ok() {
        return Err(format!("HTTP {}", response.status()).into());
    }
    let blob: web_sys::Blob = JsFuture::from(response.blob()?).await?.dyn_into()?;
    let options = web_sys::ImageBitmapOptions::new();
    options.set_premultiply_alpha(web_sys::PremultiplyAlpha::None);
    options.set_color_space_conversion(web_sys::ColorSpaceConversion::None);
    let bitmap = window.create_image_bitmap_with_blob_and_image_bitmap_options(&blob, &options)?;
    JsFuture::from(bitmap).await?.dyn_into()
}

pub struct TextureStore {
    entries: HashMap<TextureKey, TextureEntry>,
    loaded: Rc<RefCell<Vec<Loaded>>>,
    generated: HashMap<&'static str, (u32, u32, Vec<u8>)>,
    samplers: HashMap<SamplerKey, wgpu::Sampler>,
    placeholder: wgpu::TextureView,
    mipmaps: MipmapGenerator,
    asset_base: String,
    /// Bumped whenever a texture becomes ready, so materials rebuild bind groups.
    pub generation: u64,
    pub failures: Vec<String>,
}

impl TextureStore {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, asset_base: String) -> Self {
        let placeholder = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("white placeholder"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            placeholder.as_image_copy(),
            &[255; 4],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        Self {
            entries: HashMap::new(),
            loaded: Rc::default(),
            generated: HashMap::new(),
            samplers: HashMap::new(),
            placeholder: placeholder.create_view(&Default::default()),
            mipmaps: MipmapGenerator::new(device),
            asset_base,
            generation: 0,
            failures: Vec::new(),
        }
    }

    /// Start loading a texture if it is new.
    pub fn request(&mut self, texture: &TextureRef) {
        let key = TextureKey::of(texture);
        if self.entries.contains_key(&key) {
            return;
        }
        self.entries.insert(
            key.clone(),
            TextureEntry {
                texture: None,
                view: None,
                bytes: 0,
                state: TextureState::Loading,
            },
        );
        match &key.source {
            TextureSource::File(path) => {
                let url = format!("{}{}", self.asset_base, path);
                let loaded = self.loaded.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let result = fetch_bitmap(&url)
                        .await
                        .map(Pixels::Bitmap)
                        .map_err(|error| format!("{url}: {}", js_message(&error)));
                    loaded.borrow_mut().push(Loaded { key, result });
                });
            }
            TextureSource::Generated(name) => {
                if let Some((width, height, rgba)) = self.generated.get(name) {
                    self.loaded.borrow_mut().push(Loaded {
                        key,
                        result: Ok(Pixels::Raw {
                            width: *width,
                            height: *height,
                            rgba: rgba.clone(),
                        }),
                    });
                }
            }
        }
    }

    /// Provide (or replace) runtime-generated RGBA8 pixels, rows top to bottom
    /// like a canvas. Every variant of the key re-uploads on the next frame.
    pub fn set_generated(&mut self, name: &'static str, width: u32, height: u32, rgba: Vec<u8>) {
        assert_eq!(
            rgba.len(),
            (width * height * 4) as usize,
            "RGBA8 pixel count"
        );
        let keys: Vec<_> = self
            .entries
            .keys()
            .filter(|key| key.source == TextureSource::Generated(name))
            .cloned()
            .collect();
        for key in keys {
            self.loaded.borrow_mut().push(Loaded {
                key,
                result: Ok(Pixels::Raw {
                    width,
                    height,
                    rgba: rgba.clone(),
                }),
            });
        }
        self.generated.insert(name, (width, height, rgba));
    }

    /// Upload everything that finished loading. Returns true when any texture
    /// became ready. Loaded textures wait (still pending) until the mipmap blits have
    /// compiled.
    pub fn drain(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        if !self.mipmaps.ready(device) {
            return false;
        }
        let loaded: Vec<_> = self.loaded.borrow_mut().drain(..).collect();
        let changed = !loaded.is_empty();
        for Loaded { key, result } in loaded {
            let Some(entry) = self.entries.get_mut(&key) else {
                continue;
            };
            match result {
                Err(message) => {
                    entry.state = TextureState::Failed;
                    web_sys::console::error_1(&format!("Texture failed: {message}").into());
                    self.failures.push(message);
                }
                Ok(pixels) => {
                    let (width, height) = match &pixels {
                        Pixels::Bitmap(bitmap) => (bitmap.width(), bitmap.height()),
                        Pixels::Raw { width, height, .. } => (*width, *height),
                    };
                    let levels = if key.mipmaps {
                        32 - width.max(height).max(1).leading_zeros()
                    } else {
                        1
                    };
                    let format = if key.srgb {
                        wgpu::TextureFormat::Rgba8UnormSrgb
                    } else {
                        wgpu::TextureFormat::Rgba8Unorm
                    };
                    let size = wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    };
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some(match &key.source {
                            TextureSource::File(path) => path,
                            TextureSource::Generated(name) => name,
                        }),
                        size,
                        mip_level_count: levels,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::COPY_DST
                            | wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    });
                    match pixels {
                        Pixels::Bitmap(bitmap) => {
                            queue.copy_external_image_to_texture(
                                &wgpu::CopyExternalImageSourceInfo {
                                    source: wgpu::ExternalImageSource::ImageBitmap(bitmap.clone()),
                                    origin: wgpu::Origin2d::ZERO,
                                    flip_y: key.flip_y,
                                },
                                wgpu::CopyExternalImageDestInfo {
                                    texture: &texture,
                                    mip_level: 0,
                                    origin: wgpu::Origin3d::ZERO,
                                    aspect: wgpu::TextureAspect::All,
                                    color_space: wgpu::PredefinedColorSpace::Srgb,
                                    premultiplied_alpha: false,
                                },
                                size,
                            );
                            bitmap.close();
                        }
                        Pixels::Raw { rgba, .. } => {
                            // Flip rows like Three's flipY upload of a canvas.
                            let row = (width * 4) as usize;
                            let flipped = if key.flip_y {
                                let mut flipped = Vec::with_capacity(rgba.len());
                                for y in (0..height as usize).rev() {
                                    flipped.extend_from_slice(&rgba[y * row..(y + 1) * row]);
                                }
                                flipped
                            } else {
                                rgba
                            };
                            queue.write_texture(
                                texture.as_image_copy(),
                                &flipped,
                                wgpu::TexelCopyBufferLayout {
                                    offset: 0,
                                    bytes_per_row: Some(width * 4),
                                    rows_per_image: None,
                                },
                                size,
                            );
                        }
                    }
                    self.mipmaps.generate(device, queue, &texture, key.srgb);
                    if let Some(old) = entry.texture.take() {
                        old.destroy();
                    }
                    entry.bytes = (width as u64 * height as u64 * 4 * 4).div_ceil(3);
                    entry.view = Some(texture.create_view(&Default::default()));
                    entry.texture = Some(texture);
                    entry.state = TextureState::Ready;
                }
            }
        }
        if changed {
            self.generation += 1;
        }
        changed
    }

    /// The texture's view, or the white placeholder while it loads.
    pub fn view(&self, texture: &TextureRef) -> (&wgpu::TextureView, bool) {
        match self
            .entries
            .get(&TextureKey::of(texture))
            .and_then(|e| e.view.as_ref())
        {
            Some(view) => (view, true),
            None => (&self.placeholder, false),
        }
    }

    pub fn placeholder(&self) -> &wgpu::TextureView {
        &self.placeholder
    }

    pub fn sampler(
        &mut self,
        device: &wgpu::Device,
        texture: Option<&TextureRef>,
    ) -> wgpu::Sampler {
        let key = texture.map_or(
            SamplerKey {
                wrap: Wrap::Clamp,
                anisotropy: 1,
                mipmaps: false,
            },
            |t| SamplerKey {
                wrap: t.wrap,
                anisotropy: (t.anisotropy as u16).clamp(1, 16),
                mipmaps: t.mipmaps,
            },
        );
        self.samplers
            .entry(key)
            .or_insert_with(|| {
                let address = match key.wrap {
                    Wrap::Clamp => wgpu::AddressMode::ClampToEdge,
                    Wrap::Repeat => wgpu::AddressMode::Repeat,
                    Wrap::Mirror => wgpu::AddressMode::MirrorRepeat,
                };
                device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("material"),
                    address_mode_u: address,
                    address_mode_v: address,
                    address_mode_w: address,
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    // Anisotropy requires linear mip filtering even for one level.
                    mipmap_filter: if key.mipmaps || key.anisotropy > 1 {
                        wgpu::MipmapFilterMode::Linear
                    } else {
                        wgpu::MipmapFilterMode::Nearest
                    },
                    anisotropy_clamp: key.anisotropy,
                    ..Default::default()
                })
            })
            .clone()
    }

    /// Textures still downloading or awaiting generated pixels.
    pub fn pending(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.state == TextureState::Loading)
            .count()
    }

    pub fn count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| e.texture.is_some())
            .count()
    }

    pub fn bytes(&self) -> u64 {
        self.entries.values().map(|e| e.bytes).sum()
    }
}

pub fn js_message(value: &JsValue) -> String {
    value
        .dyn_ref::<js_sys::Error>()
        .map(|error| String::from(error.message()))
        .or_else(|| value.as_string())
        .unwrap_or_else(|| format!("{value:?}"))
}
