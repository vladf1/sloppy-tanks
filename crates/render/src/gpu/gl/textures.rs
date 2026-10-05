//! Browser-decoded textures and ImageData upload directly through glow. All
//! sampling uses explicit sampler objects; the upload unit is tracked separately.
use crate::gpu::gl::device::{Device, Queue, Sampler, Texture};
use glow::HasContext;
use sloppy_core::scene::{TextureRef, TextureSource, Wrap};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
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
    texture: Option<Texture>,
    view: Option<Texture>,
    bytes: u64,
    state: TextureState,
}

enum Pixels {
    Bitmap(web_sys::ImageBitmap),
    Image(web_sys::ImageData),
}

pub use super::image::image_data;

struct Loaded {
    key: TextureKey,
    result: Result<Pixels, String>,
}

async fn fetch_bitmap(url: &str, flip_y: bool) -> Result<web_sys::ImageBitmap, JsValue> {
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
    if flip_y {
        options.set_image_orientation(web_sys::ImageOrientation::FlipY);
    }
    let bitmap = window.create_image_bitmap_with_blob_and_image_bitmap_options(&blob, &options)?;
    JsFuture::from(bitmap).await?.dyn_into()
}

pub struct TextureStore {
    entries: HashMap<TextureKey, TextureEntry>,
    loaded: Rc<RefCell<Vec<Loaded>>>,
    generated: HashMap<&'static str, web_sys::ImageData>,
    samplers: HashMap<SamplerKey, Sampler>,
    placeholder: Texture,
    asset_base: String,
    pub generation: u64,
    pub failures: Vec<String>,
}
impl TextureStore {
    pub fn new(device: &Device, _queue: &Queue, asset_base: String) -> Self {
        let placeholder = Texture::new(device, glow::RGBA8, 1, 1, 1);
        unsafe {
            device.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                1,
                1,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&[255; 4])),
            );
        }
        Self {
            entries: HashMap::new(),
            loaded: Rc::default(),
            generated: HashMap::new(),
            samplers: HashMap::new(),
            placeholder,
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
                    let result = fetch_bitmap(&url, key.flip_y)
                        .await
                        .map(Pixels::Bitmap)
                        .map_err(|error| format!("{url}: {}", js_message(&error)));
                    loaded.borrow_mut().push(Loaded { key, result });
                });
            }
            TextureSource::Generated(name) => {
                if let Some(image) = self.generated.get(name) {
                    self.loaded.borrow_mut().push(Loaded {
                        key,
                        result: Ok(Pixels::Image(image.clone())),
                    });
                }
            }
        }
    }

    /// Provide (or replace) runtime-generated pixels, rows top to bottom like a
    /// canvas. Every variant of the key re-uploads on the next frame.
    pub fn set_generated(&mut self, name: &'static str, image: web_sys::ImageData) {
        let keys: Vec<_> = self
            .entries
            .keys()
            .filter(|key| key.source == TextureSource::Generated(name))
            .cloned()
            .collect();
        for key in keys {
            self.loaded.borrow_mut().push(Loaded {
                key,
                result: Ok(Pixels::Image(image.clone())),
            });
        }
        self.generated.insert(name, image);
    }

    pub fn drain(&mut self, device: &Device, _queue: &Queue) -> bool {
        let loaded: Vec<_> = self.loaded.borrow_mut().drain(..).collect();
        let changed = !loaded.is_empty();
        for Loaded { key, result } in loaded {
            let Some(entry) = self.entries.get_mut(&key) else {
                continue;
            };
            match result {
                Err(message) => {
                    entry.state = TextureState::Failed;
                    device.fail(format!("Texture failed: {message}"));
                    self.failures.push(message);
                }
                Ok(pixels) => {
                    let (width, height) = match &pixels {
                        Pixels::Bitmap(b) => (b.width(), b.height()),
                        Pixels::Image(i) => (i.width(), i.height()),
                    };
                    let levels = if key.mipmaps {
                        32 - width.max(height).max(1).leading_zeros()
                    } else {
                        1
                    };
                    let texture = Texture::new(
                        device,
                        if key.srgb {
                            glow::SRGB8_ALPHA8
                        } else {
                            glow::RGBA8
                        },
                        width,
                        height,
                        levels,
                    );
                    device.upload(&texture);
                    unsafe {
                        // ImageBitmap flipping happened at decode. ImageData observes pixel store.
                        device.gl.pixel_store_i32(
                            web_sys::WebGl2RenderingContext::UNPACK_FLIP_Y_WEBGL,
                            i32::from(matches!(&pixels, Pixels::Image(_)) && key.flip_y),
                        );
                        device.gl.pixel_store_i32(
                            web_sys::WebGl2RenderingContext::UNPACK_PREMULTIPLY_ALPHA_WEBGL,
                            0,
                        );
                        match &pixels {
                            Pixels::Bitmap(bitmap) => device.gl.tex_sub_image_2d_with_image_bitmap(
                                glow::TEXTURE_2D,
                                0,
                                0,
                                0,
                                glow::RGBA,
                                glow::UNSIGNED_BYTE,
                                bitmap,
                            ),
                            Pixels::Image(image) => device.gl.tex_sub_image_2d_with_image_data(
                                glow::TEXTURE_2D,
                                0,
                                0,
                                0,
                                glow::RGBA,
                                glow::UNSIGNED_BYTE,
                                image,
                            ),
                        }
                        device.gl.pixel_store_i32(
                            web_sys::WebGl2RenderingContext::UNPACK_FLIP_Y_WEBGL,
                            0,
                        );
                        if key.mipmaps {
                            device.gl.generate_mipmap(glow::TEXTURE_2D);
                        }
                    }
                    if let Pixels::Bitmap(bitmap) = pixels {
                        bitmap.close();
                    }
                    device.check("texture upload/mipmaps");
                    if let Some(old) = entry.texture.take() {
                        old.destroy();
                    }
                    entry.bytes = (width as u64 * height as u64 * 4 * 4).div_ceil(3);
                    entry.view = Some(texture.clone());
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
    pub fn view(&self, texture: &TextureRef) -> (&Texture, bool) {
        match self
            .entries
            .get(&TextureKey::of(texture))
            .and_then(|e| e.view.as_ref())
        {
            Some(v) => (v, true),
            None => (&self.placeholder, false),
        }
    }
    pub fn placeholder(&self) -> &Texture {
        &self.placeholder
    }
    pub fn sampler(&mut self, device: &Device, texture: Option<&TextureRef>) -> Sampler {
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
                Sampler::new(
                    device,
                    true,
                    key.mipmaps,
                    match key.wrap {
                        Wrap::Clamp => glow::CLAMP_TO_EDGE,
                        Wrap::Repeat => glow::REPEAT,
                        Wrap::Mirror => glow::MIRRORED_REPEAT,
                    },
                    false,
                    key.anisotropy as f32,
                )
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
