//! Textures: `public/` files fetched and decoded by the browser
//! (`createImageBitmap`), plus pixels generated at runtime, which arrive as a JS
//! `ImageData`; the backend uploads both straight from the browser object
//! (`copyExternalImageToTexture`, `texSubImage2D`), so generated pixels never need a
//! copy in the engine's linear memory (which never shrinks). Uploads flip Y like
//! Three's `flipY` (unless the reference clears `flip_y`), so UV (0,0) is the image's
//! bottom-left; mip levels are rendered from the level above. Until a texture
//! arrives, materials sample a white placeholder.
//!
//! Natively the files are embedded in the binary ([`set_asset_files`]), decode on a
//! thread of their own, and upload with `write_texture`.

use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex, OnceLock};
#[cfg(target_arch = "wasm32")]
use std::{cell::RefCell, rc::Rc};

use sloppy_core::scene::{TextureRef, TextureSource, Wrap};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;

use super::backend::{Frame, Gpu, MaterialBinding, Sampler, Texture, TextureView, Uploader};
use crate::effects::bakes::{TextureBake, bake_for};
use crate::effects::ground_noise::{GROUND_NOISE, GROUND_NOISE_SIZE, ground_noise_pixels};

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

struct TextureEntry {
    texture: Option<Texture>,
    bytes: u64,
    /// Still downloading or awaiting generated pixels.
    loading: bool,
}

/// Decoded pixels the backend uploads from.
pub enum Pixels {
    #[cfg(target_arch = "wasm32")]
    Bitmap(web_sys::ImageBitmap),
    Image(ImageData),
}

impl Pixels {
    pub fn size(&self) -> (u32, u32) {
        match self {
            #[cfg(target_arch = "wasm32")]
            Pixels::Bitmap(bitmap) => (bitmap.width(), bitmap.height()),
            Pixels::Image(image) => (image.width(), image.height()),
        }
    }
}

/// RGBA8 pixels, rows top first: the browser's `ImageData`.
#[cfg(target_arch = "wasm32")]
pub type ImageData = web_sys::ImageData;

/// RGBA8 pixels, rows top first, shared by every texture variant that uploads them.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct ImageData {
    width: u32,
    height: u32,
    pub pixels: Arc<[u8]>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ImageData {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

/// RGBA8 rows (top first) as the pixels a generated texture uploads from.
#[cfg(not(target_arch = "wasm32"))]
pub fn image_data(width: u32, height: u32, rgba: &[u8]) -> Result<ImageData, String> {
    if rgba.len() != width as usize * height as usize * 4 {
        return Err(format!("{} bytes for a {width}x{height} image", rgba.len()));
    }
    Ok(ImageData {
        width,
        height,
        pixels: rgba.into(),
    })
}

/// The embedded file at a `TextureSource::File` path (relative to `public/`).
#[cfg(not(target_arch = "wasm32"))]
pub type AssetFiles = fn(&str) -> Option<&'static [u8]>;

#[cfg(not(target_arch = "wasm32"))]
static ASSET_FILES: OnceLock<AssetFiles> = OnceLock::new();

/// Where native texture files come from: the binary that embeds them says so once,
/// before the renderer loads any.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_asset_files(files: AssetFiles) {
    let _ = ASSET_FILES.set(files);
}

/// The embedded file `path` decoded to RGBA8, rows top first.
#[cfg(not(target_arch = "wasm32"))]
fn decode_file(path: &str) -> Result<ImageData, String> {
    let bytes = ASSET_FILES
        .get()
        .and_then(|files| files(path))
        .ok_or_else(|| format!("{path}: not embedded"))?;
    let image = image::load_from_memory(bytes).map_err(|error| format!("{path}: {error}"))?;
    let image = image.into_rgba8();
    let (width, height) = image.dimensions();
    image_data(width, height, image.as_raw())
}

/// Finished loads, filled by the browser's promises or native decode threads.
#[derive(Clone, Default)]
struct LoadedQueue(
    #[cfg(target_arch = "wasm32")] Rc<RefCell<Vec<Loaded>>>,
    #[cfg(not(target_arch = "wasm32"))] Arc<Mutex<Vec<Loaded>>>,
);

impl LoadedQueue {
    fn push(&self, loaded: Loaded) {
        #[cfg(target_arch = "wasm32")]
        self.0.borrow_mut().push(loaded);
        #[cfg(not(target_arch = "wasm32"))]
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(loaded);
    }

    fn take(&self) -> Vec<Loaded> {
        #[cfg(target_arch = "wasm32")]
        let taken = std::mem::take(&mut *self.0.borrow_mut());
        #[cfg(not(target_arch = "wasm32"))]
        let taken = std::mem::take(
            &mut *self
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        taken
    }
}

/// RGBA8 rows (top first) as the `ImageData` a generated texture uploads from. The
/// pixels are copied into JavaScript: the upload comes on a later frame, and an
/// `ImageData` over Wasm memory would by then read whatever reused the freed bytes,
/// or nothing once the memory grew and detached its buffer.
#[cfg(target_arch = "wasm32")]
pub fn image_data(width: u32, height: u32, rgba: &[u8]) -> Result<web_sys::ImageData, JsValue> {
    let pixels = js_sys::Uint8ClampedArray::from(rgba);
    web_sys::ImageData::new_with_js_u8_clamped_array_and_sh(&pixels, width, height)
}

struct Loaded {
    key: TextureKey,
    result: Result<Pixels, String>,
}

/// WebGL cannot flip an `ImageBitmap` while uploading it, so that build decodes
/// files the reference flips upside down instead (`fetch_bitmap`).
#[cfg(target_arch = "wasm32")]
const FLIP_BITMAPS_ON_DECODE: bool = cfg!(feature = "webgl");

// Texture files decode in an image element (`HTMLImageElement.decode`), which every
// browser does off the main thread: WebKit decoded `createImageBitmap(blob)` on it, one
// file after another, about 80 ms of a village start once the arena was built. The
// bitmap made from the decoded image has the same texels. The single-player page
// starts the scene's decodes before the engine loads (`startTextureDownloads` in
// src/main.ts) and leaves them in a map by URL; the store takes each over once, so
// every browser downloads a file once. A URL the page did not start, or whose early
// decode failed, is decoded here.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = "export function textureImage(url) {
  const images = globalThis.sloppyTextureImages;
  const early = images?.get(url);
  images?.delete(url);
  const decode = () => {
    const image = new Image();
    image.src = url;
    return image.decode().then(() => image);
  };
  return early ? early.catch(decode) : decode();
}")]
extern "C" {
    #[wasm_bindgen(js_name = textureImage)]
    fn texture_image(url: &str) -> js_sys::Promise;
}

#[cfg(all(target_arch = "wasm32", not(feature = "webgl")))]
#[wasm_bindgen(inline_js = r#"export function uploadsImageData() {
  return !/\bFirefox\//.test(navigator.userAgent);
}

export function imageDataBitmap(image) {
  return createImageBitmap(image, { premultiplyAlpha: "none", colorSpaceConversion: "none" });
}"#)]
extern "C" {
    #[wasm_bindgen(js_name = uploadsImageData)]
    fn uploads_image_data() -> bool;
    #[wasm_bindgen(js_name = imageDataBitmap)]
    fn image_data_bitmap(image: &ImageData) -> js_sys::Promise;
}

#[cfg(target_arch = "wasm32")]
async fn fetch_bitmap(url: &str, flip_y: bool) -> Result<web_sys::ImageBitmap, JsValue> {
    let window = web_sys::window().ok_or("no window")?;
    let image: web_sys::HtmlImageElement = JsFuture::from(texture_image(url)).await?.dyn_into()?;
    let options = web_sys::ImageBitmapOptions::new();
    options.set_premultiply_alpha(web_sys::PremultiplyAlpha::None);
    options.set_color_space_conversion(web_sys::ColorSpaceConversion::None);
    if flip_y && FLIP_BITMAPS_ON_DECODE {
        options.set_image_orientation(web_sys::ImageOrientation::FlipY);
    }
    let bitmap = window
        .create_image_bitmap_with_html_image_element_and_image_bitmap_options(&image, &options)?;
    JsFuture::from(bitmap).await?.dyn_into()
}

pub struct TextureStore {
    entries: HashMap<TextureKey, TextureEntry>,
    loaded: LoadedQueue,
    generated: HashMap<&'static str, ImageData>,
    /// Requested textures that a bake draws (`effects::bakes`), until it has.
    bakes: Vec<TextureKey>,
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
            loaded: LoadedQueue::default(),
            generated: HashMap::new(),
            bakes: Vec::new(),
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
                loading: true,
            },
        );
        match &key.source {
            #[cfg(target_arch = "wasm32")]
            TextureSource::File(path) => {
                let url = format!("{}{}", self.asset_base, path);
                let loaded = self.loaded.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let result = fetch_bitmap(&url, key.flip_y)
                        .await
                        .map(Pixels::Bitmap)
                        .map_err(|error| format!("{url}: {}", js_message(&error)));
                    loaded.push(Loaded { key, result });
                });
            }
            #[cfg(not(target_arch = "wasm32"))]
            TextureSource::File(path) => {
                let path = format!("{}{}", self.asset_base, path);
                let loaded = self.loaded.clone();
                std::thread::spawn(move || {
                    let result = decode_file(&path).map(Pixels::Image);
                    loaded.push(Loaded { key, result });
                });
            }
            TextureSource::Generated(name) => {
                if *name == GROUND_NOISE && !self.generated.contains_key(name) {
                    let size = GROUND_NOISE_SIZE as u32;
                    match image_data(size, size, &ground_noise_pixels()) {
                        Ok(image) => {
                            self.generated.insert(GROUND_NOISE, image);
                        }
                        Err(error) => self
                            .failures
                            .push(format!("{GROUND_NOISE}: {}", js_message(&error))),
                    }
                }
                if let Some(image) = self.generated.get(name) {
                    self.loaded.push(Loaded {
                        key,
                        result: Ok(Pixels::Image(image.clone())),
                    });
                } else if bake_for(name).is_some() {
                    self.bakes.push(key);
                }
            }
        }
    }

    /// Provide (or replace) runtime-generated pixels, rows top to bottom like a
    /// canvas. Every variant of the key re-uploads on the next frame.
    pub fn set_generated(&mut self, name: &'static str, image: ImageData) {
        let keys: Vec<_> = self
            .entries
            .keys()
            .filter(|key| key.source == TextureSource::Generated(name))
            .cloned()
            .collect();
        for key in keys {
            self.loaded.push(Loaded {
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
        let loaded = self.loaded.take();
        let changed = !loaded.is_empty();
        for Loaded { key, result } in loaded {
            let Some(entry) = self.entries.get_mut(&key) else {
                continue;
            };
            // Firefox's WebGPU takes no `ImageData` as an upload's source (its
            // `GPUCopyExternalImageSourceInfo` names only bitmaps, images and canvases), so
            // there generated pixels first become a bitmap, exact without premultiplying
            // or converting colour, and come back as one. A page cannot ask the queue
            // whether it takes `ImageData` without a call that panics inside wgpu.
            #[cfg(all(target_arch = "wasm32", not(feature = "webgl")))]
            if let Ok(Pixels::Image(image)) = &result
                && !uploads_image_data()
            {
                let bitmap = image_data_bitmap(image);
                let loaded = self.loaded.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let result = JsFuture::from(bitmap)
                        .await
                        .and_then(|bitmap| bitmap.dyn_into())
                        .map(Pixels::Bitmap)
                        .map_err(|error| format!("{key:?}: {}", js_message(&error)));
                    loaded.push(Loaded { key, result });
                });
                continue;
            }
            entry.loading = false;
            match result {
                Err(message) => {
                    #[cfg(target_arch = "wasm32")]
                    web_sys::console::error_1(&format!("Texture failed: {message}").into());
                    #[cfg(not(target_arch = "wasm32"))]
                    eprintln!("Texture failed: {message}");
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
                        #[cfg(target_arch = "wasm32")]
                        Pixels::Bitmap(_) => key.flip_y && !FLIP_BITMAPS_ON_DECODE,
                        Pixels::Image(_) => key.flip_y,
                    };
                    // Replacing the entry's texture drops (destroys) the old one.
                    entry.texture = Some(self.uploader.upload(gpu, &key, &pixels, levels, flip_y));
                    // Both backends copy the pixels at once, so a bitmap can go now.
                    #[cfg(target_arch = "wasm32")]
                    if let Pixels::Bitmap(bitmap) = pixels {
                        bitmap.close();
                    }
                    entry.bytes = (width as u64 * height as u64 * 4 * 4).div_ceil(3);
                }
            }
        }
        if changed {
            self.generation += 1;
        }
        changed
    }

    /// The texture's view, or the white placeholder while it loads or for none.
    pub fn view(&self, texture: Option<&TextureRef>) -> &TextureView {
        texture
            .and_then(|texture| self.entries.get(&TextureKey::of(texture)))
            .and_then(|entry| entry.texture.as_ref())
            .map_or(self.uploader.placeholder(), |texture| &texture.view)
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

    /// The requested textures a bake draws (`effects::bakes`) that it has not yet.
    pub fn bakes_due(&self) -> Vec<(TextureKey, &'static TextureBake)> {
        self.bakes
            .iter()
            .filter_map(|key| match key.source {
                TextureSource::Generated(name) => Some((key.clone(), bake_for(name)?)),
                TextureSource::File(_) => None,
            })
            .collect()
    }

    /// Whether `bake` can draw now: its pipeline and the mip blits are ready. The
    /// first ask starts its background compile.
    pub fn bake_ready(&mut self, gpu: &Gpu, bake: &'static TextureBake) -> bool {
        self.uploader.ready(gpu) && self.uploader.ready_to_bake(gpu, bake)
    }

    /// Whether `texture` has arrived or failed (it no longer waits).
    pub fn loaded(&self, texture: &TextureRef) -> bool {
        self.entries
            .get(&TextureKey::of(texture))
            .is_some_and(|entry| !entry.loading)
    }

    /// Draw `bake` into its texture `key` and render its mip levels, reading the frame
    /// and the binding of its source's material when it has one; it must be ready
    /// ([`Self::bake_ready`]).
    pub fn bake(
        &mut self,
        gpu: &Gpu,
        key: &TextureKey,
        bake: &'static TextureBake,
        inputs: Option<(&Frame, &MaterialBinding)>,
    ) {
        self.bakes.retain(|due| due != key);
        if let Some(entry) = self.entries.get_mut(key) {
            entry.texture = Some(self.uploader.bake(gpu, bake, inputs));
            entry.bytes = bake.bytes();
            entry.loading = false;
            self.generation += 1;
        }
    }

    /// Textures still downloading or awaiting generated pixels.
    pub fn pending(&self) -> usize {
        self.entries.values().filter(|entry| entry.loading).count()
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

#[cfg(target_arch = "wasm32")]
pub fn js_message(value: &JsValue) -> String {
    value
        .dyn_ref::<js_sys::Error>()
        .map(|error| String::from(error.message()))
        .or_else(|| value.as_string())
        .unwrap_or_else(|| format!("{value:?}"))
}

/// Native errors are already messages.
#[cfg(not(target_arch = "wasm32"))]
fn js_message(message: &str) -> String {
    message.to_owned()
}
