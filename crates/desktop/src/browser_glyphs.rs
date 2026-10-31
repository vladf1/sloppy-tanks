//! A page's text drawn by the browser: every grapheme cluster egui lays out in the UI
//! or display family is drawn with a 2D canvas's `fillText` in the production page's
//! CSS font stacks (`src/style.css`'s `--font-ui` and `--font-display`, on `main`), so a
//! Mac shows San Francisco and Impact as the DOM page did, in the browser's own
//! rasteriser. egui keeps its installed fonts (`theme.rs`) for line metrics; each
//! cluster's advance comes from the browser's `measureText`, unkerned, as egui asks for
//! one cluster at a time. The stacks end in the game's own faces (registered with the
//! document under `Sloppy Display` and `Sloppy UI` from the bytes `index.html` fetched),
//! so a system without Impact draws Anton Wide as the egui page does rather than a
//! generic sans.

use std::cell::RefCell;

use egui::epaint::text::{
    FontPriority, GlyphBitmap, GlyphRasterizer, GlyphRasterizerRequest, RasterizedGlyph,
};
use egui::{Color32, ColorImage, FontFamily};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::wasm_bindgen;

/// `--font-display`, with the game's Anton Wide before the generic family.
const DISPLAY_STACK: &str = "Impact, Haettenschweiler, \"Arial Narrow Bold\", \
     \"Franklin Gothic Bold\", \"Sloppy Display\", sans-serif";
/// `--font-ui`, with the game's Roboto before the generic family (Chrome resolves
/// `system-ui` everywhere, so it shows only where a browser has no system UI face).
const UI_STACK: &str = "system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", Roboto, \
     \"Helvetica Neue\", Arial, \"Sloppy UI\", sans-serif";
/// The weight of the `bold` family: Roboto Bold's, as egui lays it out.
const BOLD_WEIGHT: u16 = 700;
/// Anton Wide at Impact's capital height (`theme.rs`'s `ANTON_SCALE`), as CSS
/// `size-adjust`.
const ANTON_SIZE_ADJUST: &str = "97.5%";
/// Transparent pixels around a glyph's ink, for antialiasing that `measureText`'s
/// bounding box leaves out.
const PADDING_PX: i32 = 2;

#[wasm_bindgen]
extern "C" {
    /// A `CanvasRenderingContext2D` or an `OffscreenCanvasRenderingContext2D`: the
    /// calls made here are the same on both.
    type Context2d;
    #[wasm_bindgen(method, setter = font)]
    fn set_font(this: &Context2d, font: &str);
    #[wasm_bindgen(method, setter = fillStyle)]
    fn set_fill_style(this: &Context2d, style: &str);
    #[wasm_bindgen(method, setter = textBaseline)]
    fn set_text_baseline(this: &Context2d, baseline: &str);
    #[wasm_bindgen(method, js_name = measureText)]
    fn measure_text(this: &Context2d, text: &str) -> web_sys::TextMetrics;
    #[wasm_bindgen(method, js_name = clearRect)]
    fn clear_rect(this: &Context2d, x: f64, y: f64, width: f64, height: f64);
    #[wasm_bindgen(method, js_name = setTransform)]
    fn set_transform(this: &Context2d, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64);
    #[wasm_bindgen(method, js_name = fillText)]
    fn fill_text(this: &Context2d, text: &str, x: f64, y: f64);
    #[wasm_bindgen(method, js_name = getImageData)]
    fn get_image_data(
        this: &Context2d,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> web_sys::ImageData;
    #[wasm_bindgen(method, getter)]
    fn canvas(this: &Context2d) -> Canvas;

    /// A `<canvas>` or an `OffscreenCanvas`.
    type Canvas;
    #[wasm_bindgen(method, getter)]
    fn width(this: &Canvas) -> u32;
    #[wasm_bindgen(method, setter)]
    fn set_width(this: &Canvas, width: u32);
    #[wasm_bindgen(method, getter)]
    fn height(this: &Canvas) -> u32;
    #[wasm_bindgen(method, setter)]
    fn set_height(this: &Canvas, height: u32);
}

struct Rasterizer {
    context: Context2d,
    window: web_sys::Window,
    /// The font the context has, so a run of one size sets it once.
    font: String,
    performance: Option<web_sys::Performance>,
    /// `globalThis.sloppyGlyphs`: how many clusters were drawn and the milliseconds it
    /// took, for measurements.
    stats: js_sys::Object,
    count: u32,
    milliseconds: f64,
}

thread_local! {
    static RASTERIZER: RefCell<Option<Rasterizer>> = const { RefCell::new(None) };
}

/// Draw `context`'s UI and display text with the browser from the next pass on.
pub fn install(context: &egui::Context) {
    let ready = RASTERIZER.with_borrow_mut(|rasterizer| {
        if rasterizer.is_none() {
            register_embedded_faces();
            *rasterizer = Rasterizer::new();
        }
        rasterizer.is_some()
    });
    if !ready {
        return;
    }
    context.add_glyph_rasterizer(
        GlyphRasterizer::new("sloppy::browser_glyphs", rasterize)
            .with_priority(FontPriority::Highest),
    );
    // The canvas's coverage already carries the browser's own text contrast, as the DOM
    // page's text did: egui's dark-mode boost would embolden it again.
    let mut style = (*context.global_style()).clone();
    style.visuals.text_options.color_transfer_function =
        egui::epaint::FontColorTransferFunction::Off;
    context.set_global_style(style);
}

/// The game's faces under the names the stacks end in, from the bytes the page fetched.
fn register_embedded_faces() {
    let Some(fonts) = web_sys::window()
        .and_then(|window| window.document())
        .map(|document| document.fonts())
    else {
        return;
    };
    let faces = [
        ("Sloppy UI", "fonts/Roboto-Regular.ttf", "400", None),
        ("Sloppy UI", "fonts/Roboto-Bold.ttf", "700", None),
        (
            "Sloppy Display",
            "fonts/Anton-Wide.ttf",
            "400",
            Some(ANTON_SIZE_ADJUST),
        ),
    ];
    for (family, path, weight, size_adjust) in faces {
        let Some(bytes) = crate::assets::load(path).ready() else {
            report!("{path} is missing: the browser's text falls back past it");
            continue;
        };
        let descriptors = web_sys::FontFaceDescriptors::new();
        descriptors.set_weight(weight);
        if let Some(size_adjust) = size_adjust {
            let _ = js_sys::Reflect::set(&descriptors, &"sizeAdjust".into(), &size_adjust.into());
        }
        let source = js_sys::Uint8Array::from(bytes.as_ref());
        match web_sys::FontFace::new_with_array_buffer_view_and_descriptors(
            family,
            &source,
            &descriptors,
        ) {
            Ok(face) => {
                if let Err(error) = fonts.add(&face) {
                    report!("{family} ({path}) was not added to the page: {error:?}");
                }
            }
            Err(error) => report!("{family} ({path}) did not load: {error:?}"),
        }
    }
}

impl Rasterizer {
    fn new() -> Option<Self> {
        let options = js_sys::Object::new();
        js_sys::Reflect::set(&options, &"willReadFrequently".into(), &true.into()).ok()?;
        let context = match web_sys::OffscreenCanvas::new(64, 64) {
            Ok(canvas) => canvas.get_context_with_context_options("2d", &options),
            Err(_) => {
                let document = web_sys::window()?.document()?;
                let canvas: web_sys::HtmlCanvasElement =
                    document.create_element("canvas").ok()?.dyn_into().ok()?;
                canvas.get_context_with_context_options("2d", &options)
            }
        }
        .ok()
        .flatten()?
        .unchecked_into::<Context2d>();
        context.set_text_baseline("alphabetic");
        let stats = js_sys::Object::new();
        js_sys::Reflect::set(&js_sys::global(), &"sloppyGlyphs".into(), &stats).ok()?;
        let window = web_sys::window()?;
        Some(Self {
            context,
            font: String::new(),
            performance: window.performance(),
            window,
            stats,
            count: 0,
            milliseconds: 0.0,
        })
    }

    fn now(&self) -> f64 {
        self.performance.as_ref().map_or(0.0, |p| p.now())
    }

    fn draw(&mut self, request: &GlyphRasterizerRequest<'_>) -> Option<RasterizedGlyph> {
        let (weight, stack) = match request.family {
            FontFamily::Proportional => (400, UI_STACK),
            FontFamily::Name(name) if &**name == "bold" => (BOLD_WEIGHT, UI_STACK),
            FontFamily::Name(name) if &**name == "display" => (400, DISPLAY_STACK),
            _ => return None,
        };
        // An emoji stays with egui's fonts: the canvas would draw it in colour, which
        // egui would then tint as text.
        if egui::epaint::text::has_emoji_presentation(request.cluster) {
            return None;
        }
        // The font in CSS pixels, drawn scaled to the device's: a size picks its optical
        // size (San Francisco's Text or Display cut, with their tracking) by CSS pixels,
        // as the DOM page's text did, not by the physical pixels egui asks for.
        let scale = self.window.device_pixel_ratio().max(1.0);
        let font = format!(
            "{weight} {}px {stack}",
            f64::from(request.font_size_px) / scale
        );
        if font != self.font {
            self.context.set_font(&font);
            // The sizes asked for, for measurements (`sloppyGlyphs.sizes`).
            let size = format!("{weight}/{}", request.font_size_px);
            let sizes = js_sys::Reflect::get(&self.stats, &"sizes".into())
                .ok()
                .and_then(|sizes| sizes.dyn_into::<js_sys::Array>().ok())
                .unwrap_or_else(|| {
                    let sizes = js_sys::Array::new();
                    let _ = js_sys::Reflect::set(&self.stats, &"sizes".into(), &sizes);
                    sizes
                });
            if !sizes.includes(&size.as_str().into(), 0) {
                sizes.push(&size.into());
            }
            self.font = font;
        }
        let metrics = self.context.measure_text(request.cluster);
        let advance_px = (metrics.width() * scale) as f32;
        let x = f64::from(request.subpixel_offset_px);
        let ink_left = metrics.actual_bounding_box_left() * scale;
        let ink_right = metrics.actual_bounding_box_right() * scale;
        let ink_up = metrics.actual_bounding_box_ascent() * scale;
        let ink_down = metrics.actual_bounding_box_descent() * scale;
        let left = (x - ink_left).floor() as i32 - PADDING_PX;
        let right = (x + ink_right).ceil() as i32 + PADDING_PX;
        let top = (-ink_up).floor() as i32 - PADDING_PX;
        let bottom = ink_down.ceil() as i32 + PADDING_PX;
        let width = right - left;
        let height = bottom - top;
        // A space has no ink, but egui still needs a bitmap to take its advance.
        let inked = ink_left + ink_right > 0.0 && ink_up + ink_down > 0.0;
        if !inked || width <= 0 || height <= 0 {
            return Some(RasterizedGlyph {
                bitmap: GlyphBitmap {
                    image: ColorImage::filled([1, 1], Color32::TRANSPARENT),
                    offset_px: egui::Vec2::ZERO,
                    is_color: false,
                },
                advance_px,
            });
        }
        let canvas = self.context.canvas();
        if canvas.width() < width as u32 || canvas.height() < height as u32 {
            // Resizing resets the context's state.
            canvas.set_width(canvas.width().max(width as u32));
            canvas.set_height(canvas.height().max(height as u32));
            self.context.set_text_baseline("alphabetic");
            self.context.set_font(&self.font);
        }
        self.context.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        self.context
            .clear_rect(0.0, 0.0, f64::from(width), f64::from(height));
        self.context.set_fill_style("#fff");
        self.context
            .set_transform(scale, 0.0, 0.0, scale, x - f64::from(left), -f64::from(top));
        self.context.fill_text(request.cluster, 0.0, 0.0);
        let data = self.context.get_image_data(0, 0, width, height).data().0;
        let pixels = data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|[_, _, _, alpha]| Color32::from_white_alpha(*alpha))
            .collect();
        Some(RasterizedGlyph {
            bitmap: GlyphBitmap {
                image: ColorImage::new([width as usize, height as usize], pixels),
                offset_px: egui::vec2(left as f32, top as f32),
                is_color: false,
            },
            advance_px,
        })
    }
}

fn rasterize(request: &GlyphRasterizerRequest<'_>) -> Option<RasterizedGlyph> {
    RASTERIZER.with_borrow_mut(|rasterizer| {
        let rasterizer = rasterizer.as_mut()?;
        let start = rasterizer.now();
        let glyph = rasterizer.draw(request);
        rasterizer.milliseconds += rasterizer.now() - start;
        rasterizer.count += 1;
        let _ = js_sys::Reflect::set(&rasterizer.stats, &"count".into(), &rasterizer.count.into());
        let _ = js_sys::Reflect::set(
            &rasterizer.stats,
            &"ms".into(),
            &rasterizer.milliseconds.into(),
        );
        glyph
    })
}
