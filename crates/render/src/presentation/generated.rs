//! Generated textures scenery names by key (`TextureSource::Generated`): the
//! quarry soil, baked in Rust, and the signs and harbor labels, drawn by the
//! browser's Canvas 2D (its fonts) from `effects_scenery::canvas_texture` and read
//! back as an `ImageData`.
//!
//! The soil takes about 0.6 s of Wasm, so the page claims it
//! ([`GeneratedTextures::claim_bake`]) and bakes it in a worker running the engine's
//! `bake_texture` while the main thread builds the arena and its pipelines; the
//! pixels come back through [`GeneratedTextures::supply`] and go to the GPU from JS
//! memory, never copied into this instance's heap. A page that claims nothing (the
//! labs), or whose worker fails, bakes it here in row bands between preparation
//! steps instead.

use std::collections::HashSet;

use sloppy_core::models::effects_scenery::QUARRY_SOIL_TEXTURE;
use sloppy_core::models::{QUARRY_SOIL_SIZE, bake_quarry_soil, sand_accum};

/// Soil rows baked per step: a 2048-row bake is ~300 ms natively, so 128 rows
/// keep each step near 20 ms while the loading screen yields between steps.
pub const SOIL_ROWS_PER_STEP: usize = 128;

/// An in-progress banded soil bake.
#[derive(Default)]
pub struct SoilBake {
    rows: usize,
    pixels: Vec<u8>,
    /// Handed back by the page after its workers failed: it stays here, or the
    /// page, which offers the bake before every preparation step, would hand it
    /// to failing workers again and again and the arena would never be ready.
    returned: bool,
}

impl SoilBake {
    /// Bake the next band; returns the finished pixels after the last one.
    pub fn step(&mut self, rows: usize) -> Option<Vec<u8>> {
        let end = (self.rows + rows.max(1)).min(QUARRY_SOIL_SIZE);
        if self.pixels.is_empty() {
            self.pixels
                .reserve_exact(QUARRY_SOIL_SIZE * QUARRY_SOIL_SIZE * 4);
        }
        self.pixels
            .extend_from_slice(&bake_quarry_soil(sand_accum(), self.rows, end));
        self.rows = end;
        (self.rows == QUARRY_SOIL_SIZE).then(|| std::mem::take(&mut self.pixels))
    }
}

/// Generated textures presentation has supplied or is baking.
#[derive(Default)]
pub struct GeneratedTextures {
    /// Keys already supplied or baking (read by the browser adapter).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    done: HashSet<&'static str>,
    soil: Option<SoilBake>,
    /// A bake the page runs off the main thread, awaiting [`Self::supply`].
    elsewhere: Option<&'static str>,
}

impl GeneratedTextures {
    /// Whether a band of work remains here (the soil bake).
    pub fn busy(&self) -> bool {
        self.soil.is_some()
    }

    /// Whether a bake is still in progress, here or in the page's worker (its
    /// texture is not uploaded yet).
    pub fn baking(&self) -> bool {
        self.soil.is_some() || self.elsewhere.is_some()
    }

    /// Whether only the page's worker is baking.
    pub fn baking_elsewhere(&self) -> bool {
        self.elsewhere.is_some() && self.soil.is_none()
    }

    /// Hand a bake that has not started to the page, which runs `bake_texture(key)`
    /// off the main thread and returns the pixels through [`Self::supply`].
    pub fn claim_bake(&mut self) -> Option<&'static str> {
        if self.elsewhere.is_some()
            || self
                .soil
                .as_ref()
                .is_none_or(|bake| bake.rows > 0 || bake.returned)
        {
            return None;
        }
        self.soil = None;
        self.elsewhere = Some(QUARRY_SOIL_TEXTURE);
        self.elsewhere
    }

    /// Take a claimed bake back (the page's worker failed): bake it here.
    pub fn release_bake(&mut self, key: &str) {
        if self.elsewhere == Some(key) {
            self.elsewhere = None;
            self.soil = Some(SoilBake {
                returned: true,
                ..SoilBake::default()
            });
        }
    }

    /// The claimed key the page's pixels answer, once; `None` for a key not awaited.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn take_supplied(&mut self, key: &str) -> Option<&'static str> {
        if self.elsewhere == Some(key) {
            self.elsewhere.take()
        } else {
            None
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use crate::gpu::{Renderer, image_data};
    use sloppy_core::models::effects_scenery::{CanvasOp, CanvasTexture, canvas_texture};
    use sloppy_core::models::node_textures;
    use sloppy_core::scene::{Node, TextureSource};
    use wasm_bindgen::JsCast;

    impl GeneratedTextures {
        /// Supply the canvas textures a scenery tree uses now, and start baking
        /// its soil (finished by [`Self::step`]).
        pub fn request_scenery(&mut self, renderer: &mut Renderer, root: &Node) {
            for source in node_textures(root) {
                let TextureSource::Generated(key) = source else {
                    continue;
                };
                if !self.done.insert(key) {
                    continue;
                }
                if key == QUARRY_SOIL_TEXTURE {
                    self.soil = Some(SoilBake::default());
                } else if let Some(canvas) = canvas_texture(key) {
                    match draw_canvas(&canvas) {
                        Ok(image) => renderer.set_generated_texture(key, image),
                        Err(error) => web_sys::console::error_1(
                            &format!("canvas texture {key}: {error:?}").into(),
                        ),
                    }
                }
            }
        }

        /// Upload the pixels of a claimed bake (RGBA8 rows, top first). Returns
        /// whether the key was awaited; a stale result is dropped.
        pub fn supply(
            &mut self,
            renderer: &mut Renderer,
            key: &str,
            rgba: &js_sys::Uint8Array,
        ) -> Result<bool, String> {
            let size = QUARRY_SOIL_SIZE as u32;
            if rgba.length() != size * size * 4 {
                return Err(format!("{key}: {} bytes baked", rgba.length()));
            }
            // A view of the same bytes, not a copy.
            let pixels = js_sys::Uint8ClampedArray::new_with_byte_offset_and_length(
                &rgba.buffer(),
                rgba.byte_offset(),
                rgba.length(),
            );
            let image =
                web_sys::ImageData::new_with_js_u8_clamped_array_and_sh(&pixels, size, size)
                    .map_err(|error| format!("{key}: {error:?}"))?;
            let Some(key) = self.take_supplied(key) else {
                return Ok(false);
            };
            renderer.set_generated_texture(key, image);
            Ok(true)
        }

        /// Bake one band of pending soil; uploads it once complete.
        pub fn step(&mut self, renderer: &mut Renderer, rows: usize) {
            if let Some(bake) = &mut self.soil
                && let Some(pixels) = bake.step(rows)
            {
                let size = QUARRY_SOIL_SIZE as u32;
                match image_data(size, size, &pixels) {
                    Ok(image) => renderer.set_generated_texture(QUARRY_SOIL_TEXTURE, image),
                    Err(error) => web_sys::console::error_1(
                        &format!("{QUARRY_SOIL_TEXTURE}: {error:?}").into(),
                    ),
                }
                self.soil = None;
            }
        }
    }

    /// Replay the Canvas 2D steps on a fresh, transparent `OffscreenCanvas` and
    /// read back its RGBA rows (top first, straight alpha).
    fn draw_canvas(canvas: &CanvasTexture) -> Result<web_sys::ImageData, wasm_bindgen::JsValue> {
        let surface = web_sys::OffscreenCanvas::new(canvas.width, canvas.height)?;
        let context: web_sys::OffscreenCanvasRenderingContext2d = surface
            .get_context("2d")?
            .ok_or("no 2d context")?
            .dyn_into()?;
        for op in &canvas.ops {
            match op {
                CanvasOp::FillRect { color, rect } => {
                    context.set_fill_style_str(color);
                    context.fill_rect(rect[0], rect[1], rect[2], rect[3]);
                }
                CanvasOp::StrokeRect {
                    color,
                    line_width,
                    rect,
                } => {
                    context.set_stroke_style_str(color);
                    context.set_line_width(*line_width);
                    context.stroke_rect(rect[0], rect[1], rect[2], rect[3]);
                }
                CanvasOp::FillText {
                    color,
                    font,
                    align,
                    baseline,
                    text,
                    at,
                    max_width,
                } => {
                    context.set_fill_style_str(color);
                    context.set_font(font);
                    context.set_text_align(align);
                    context.set_text_baseline(baseline);
                    match max_width {
                        Some(width) => {
                            context.fill_text_with_max_width(text, at[0], at[1], *width)?
                        }
                        None => context.fill_text(text, at[0], at[1])?,
                    }
                }
                CanvasOp::FillPolygon { color, points } => {
                    context.set_fill_style_str(color);
                    context.begin_path();
                    for (index, point) in points.iter().enumerate() {
                        if index == 0 {
                            context.move_to(point[0], point[1]);
                        } else {
                            context.line_to(point[0], point[1]);
                        }
                    }
                    context.fill();
                }
            }
        }
        context.get_image_data(0.0, 0.0, canvas.width as f64, canvas.height as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::models::quarry_soil_pixels;

    #[test]
    fn the_page_claims_a_soil_bake_once_before_it_starts() {
        let mut textures = GeneratedTextures {
            soil: Some(SoilBake::default()),
            ..GeneratedTextures::default()
        };
        assert_eq!(textures.claim_bake(), Some(QUARRY_SOIL_TEXTURE));
        assert!(textures.baking() && textures.baking_elsewhere() && !textures.busy());
        assert_eq!(textures.claim_bake(), None);
        assert_eq!(textures.take_supplied("sign"), None);
        assert_eq!(
            textures.take_supplied(QUARRY_SOIL_TEXTURE),
            Some(QUARRY_SOIL_TEXTURE)
        );
        assert!(!textures.baking());
        assert_eq!(
            textures.take_supplied(QUARRY_SOIL_TEXTURE),
            None,
            "supplied once"
        );
    }

    #[test]
    fn a_released_or_started_bake_stays_on_the_main_thread() {
        let mut textures = GeneratedTextures {
            soil: Some(SoilBake::default()),
            ..GeneratedTextures::default()
        };
        textures.claim_bake();
        textures.release_bake(QUARRY_SOIL_TEXTURE);
        assert!(textures.busy() && !textures.baking_elsewhere());
        assert_eq!(textures.claim_bake(), None, "handed back");
        textures.soil.as_mut().unwrap().step(SOIL_ROWS_PER_STEP);
        assert_eq!(textures.claim_bake(), None, "half-baked here");
        assert!(GeneratedTextures::default().claim_bake().is_none());
    }

    #[test]
    fn banded_soil_matches_the_whole_bake() {
        let mut bake = SoilBake::default();
        let mut result = None;
        let mut steps = 0;
        while result.is_none() {
            result = bake.step(SOIL_ROWS_PER_STEP * 3 + 7);
            steps += 1;
        }
        assert_eq!(steps, QUARRY_SOIL_SIZE.div_ceil(SOIL_ROWS_PER_STEP * 3 + 7));
        assert!(result.unwrap() == quarry_soil_pixels());
    }
}
