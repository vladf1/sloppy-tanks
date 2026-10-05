//! Textures: `public/` files fetched and decoded by the browser
//! (`createImageBitmap`), plus pixels generated at runtime, which arrive as a JS
//! `ImageData`; the backend uploads both straight from the browser object
//! (`copyExternalImageToTexture`, `texSubImage2D`), so generated pixels never need a
//! copy in the engine's linear memory (which never shrinks). Uploads flip Y like
//! Three's `flipY` (unless the reference clears `flip_y`), so UV (0,0) is the image's
//! bottom-left; mip levels are rendered from the level above. Until a texture
//! arrives, materials sample a white placeholder.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use sloppy_core::scene::{TextureRef, TextureSource, Wrap};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::backend::{Gpu, Sampler, Texture, TextureView, Uploader};

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

/// What a material sampler filters and wraps with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SamplerKey {
    pub wrap: Wrap,
    pub anisotropy: u16,
    pub mipmaps: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureState {
    Loading,
    Ready,
    Failed,
}

struct TextureEntry {
    texture: Option<Texture>,
    bytes: u64,
    state: TextureState,
}

/// Decoded pixels the backend uploads from.
pub enum Pixels {
    Bitmap(web_sys::ImageBitmap),
    Image(web_sys::ImageData),
}

impl Pixels {
    pub fn size(&self) -> (u32, u32) {
        match self {
            Pixels::Bitmap(bitmap) => (bitmap.width(), bitmap.height()),
            Pixels::Image(image) => (image.width(), image.height()),
        }
    }
}

/// RGBA8 rows (top first) as the `ImageData` a generated texture uploads from.
pub fn image_data(width: u32, height: u32, rgba: &[u8]) -> Result<web_sys::ImageData, JsValue> {
    web_sys::ImageData::new_with_u8_clamped_array_and_sh(wasm_bindgen::Clamped(rgba), width, height)
}

struct Loaded {
    key: TextureKey,
    result: Result<Pixels, String>,
}

/// WebGL cannot flip an `ImageBitmap` while uploading it, so that build decodes
/// files the reference flips upside down instead (`fetch_bitmap`).
const FLIP_BITMAPS_ON_DECODE: bool = cfg!(feature = "webgl");

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
    if flip_y && FLIP_BITMAPS_ON_DECODE {
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
    uploader: Uploader,
    asset_base: String,
    /// Bumped whenever a texture becomes ready, so materials rebuild bind groups.
    pub generation: u64,
    pub failures: Vec<String>,
}

impl TextureStore {
    pub fn new(gpu: &Gpu, asset_base: String) -> Self {
        Self {
            entries: HashMap::new(),
            loaded: Rc::default(),
            generated: HashMap::new(),
            samplers: HashMap::new(),
            uploader: Uploader::new(gpu),
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

    /// Upload everything that finished loading. Returns true when any texture
    /// became ready. Loaded textures wait (still pending) until the backend's mipmap
    /// blits have compiled.
    pub fn drain(&mut self, gpu: &Gpu) -> bool {
        if !self.uploader.ready(gpu) {
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
                    let (width, height) = pixels.size();
                    let levels = if key.mipmaps {
                        32 - width.max(height).max(1).leading_zeros()
                    } else {
                        1
                    };
                    let flip_y = match &pixels {
                        Pixels::Bitmap(_) => key.flip_y && !FLIP_BITMAPS_ON_DECODE,
                        Pixels::Image(_) => key.flip_y,
                    };
                    // Replacing the entry's texture drops (destroys) the old one.
                    entry.texture = Some(self.uploader.upload(gpu, &key, &pixels, levels, flip_y));
                    // Both backends copy the pixels at once, so a bitmap can go now.
                    if let Pixels::Bitmap(bitmap) = pixels {
                        bitmap.close();
                    }
                    entry.bytes = (width as u64 * height as u64 * 4 * 4).div_ceil(3);
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
    pub fn view(&self, texture: &TextureRef) -> (&TextureView, bool) {
        match self
            .entries
            .get(&TextureKey::of(texture))
            .and_then(|e| e.texture.as_ref())
        {
            Some(texture) => (&texture.view, true),
            None => (self.uploader.placeholder(), false),
        }
    }

    pub fn placeholder(&self) -> &TextureView {
        self.uploader.placeholder()
    }

    // A wgpu sampler is a handle that clones; a WebGL one is a `Copy` name.
    #[allow(clippy::clone_on_copy)]
    pub fn sampler(&mut self, gpu: &Gpu, texture: Option<&TextureRef>) -> Sampler {
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
            .or_insert_with(|| Uploader::sampler(gpu, key))
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
