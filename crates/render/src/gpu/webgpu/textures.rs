//! WebGPU textures: `copyExternalImageToTexture` uploads of decoded files and
//! generated `ImageData`, mip levels rendered from the level above, and samplers.

use sloppy_core::scene::{TextureSource, Wrap};

use super::Gpu;
use super::context::texture_2d;
use super::pipelines::pipeline_layout;
use super::precompile::{Background, LayoutKind, PipelineSpec};
use super::resources::MIPMAP_SOURCE_ENTRIES;
use crate::gpu::textures::{Pixels, SamplerKey, TextureKey};
use crate::shader::MIPMAP_WGSL;

/// An uploaded texture with its mip chain; dropping it destroys it.
pub struct Texture {
    texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl Drop for Texture {
    fn drop(&mut self) {
        self.texture.destroy();
    }
}

pub type TextureView = wgpu::TextureView;
pub type Sampler = wgpu::Sampler;

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

/// Uploads textures and renders their mip levels; holds the white placeholder that
/// materials sample until their textures arrive.
pub struct Uploader {
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    /// The sRGB and linear blit specs while they compile in the background.
    pending: Option<(PipelineSpec, PipelineSpec, Background)>,
    /// The sRGB and linear blit pipelines.
    pipelines: Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)>,
    sampler: wgpu::Sampler,
    placeholder: wgpu::TextureView,
}

impl Uploader {
    /// Starts compiling both blit pipelines in the background; [`Self::ready`]
    /// creates them once compiled.
    pub fn new(gpu: &Gpu) -> Self {
        let device = &gpu.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mipmap source"),
            entries: MIPMAP_SOURCE_ENTRIES,
        });
        let pipeline_layout = pipeline_layout(device, "mipmap", &[&layout]);
        let srgb = mipmap_spec(wgpu::TextureFormat::Rgba8UnormSrgb);
        let linear = mipmap_spec(wgpu::TextureFormat::Rgba8Unorm);
        let compile = Background::start(device, &[(&srgb, MIPMAP_WGSL), (&linear, MIPMAP_WGSL)]);
        let placeholder = texture_2d(
            device,
            "white placeholder",
            1,
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            1,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        gpu.queue.write_texture(
            placeholder.as_image_copy(),
            &[255; 4],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: None,
            },
            placeholder.size(),
        );
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
            placeholder: placeholder.create_view(&Default::default()),
        }
    }

    /// Whether the blit pipelines exist, creating them once compiled; uploads wait
    /// until then.
    pub fn ready(&mut self, gpu: &Gpu) -> bool {
        let device = &gpu.device;
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

    /// The white placeholder.
    pub fn placeholder(&self) -> &wgpu::TextureView {
        &self.placeholder
    }

    /// Upload `pixels` as a texture with `levels` mip levels; flips Y while copying
    /// when `flip_y`.
    pub fn upload(
        &self,
        gpu: &Gpu,
        key: &TextureKey,
        pixels: &Pixels,
        levels: u32,
        flip_y: bool,
    ) -> Texture {
        let (width, height) = pixels.size();
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
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
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
        let source = match pixels {
            Pixels::Bitmap(bitmap) => wgpu::ExternalImageSource::ImageBitmap(bitmap.clone()),
            Pixels::Image(image) => wgpu::ExternalImageSource::ImageData(image.clone()),
        };
        gpu.queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo {
                source,
                origin: wgpu::Origin2d::ZERO,
                flip_y,
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
        self.generate_mipmaps(gpu, &texture, key.srgb);
        Texture {
            view: texture.create_view(&Default::default()),
            texture,
        }
    }

    fn generate_mipmaps(&self, gpu: &Gpu, texture: &wgpu::Texture, srgb: bool) {
        let device = &gpu.device;
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
        gpu.queue.submit([encoder.finish()]);
    }

    pub fn sampler(gpu: &Gpu, key: SamplerKey) -> wgpu::Sampler {
        let address = match key.wrap {
            Wrap::Clamp => wgpu::AddressMode::ClampToEdge,
            Wrap::Repeat => wgpu::AddressMode::Repeat,
            Wrap::Mirror => wgpu::AddressMode::MirrorRepeat,
        };
        gpu.device.create_sampler(&wgpu::SamplerDescriptor {
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
    }
}
