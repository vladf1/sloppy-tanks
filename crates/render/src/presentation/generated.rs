//! Generated textures scenery names by key (`TextureSource::Generated`): the
//! quarry soil, baked in Rust in row bands so no frame stalls on it, and the
//! signs and harbor labels, drawn by the browser's Canvas 2D (its fonts) from
//! `effects_scenery::canvas_texture` and read back as RGBA.

use std::collections::HashSet;

use sloppy_core::models::effects_scenery::{QUARRY_SOIL_TEXTURE, canvas_texture};
use sloppy_core::models::{QUARRY_SOIL_SIZE, bake_quarry_soil, sand_accum};
use sloppy_core::scene::{Node, TextureRef, TextureSource};

/// Soil rows baked per step: a 2048-row bake is ~300 ms natively, so 128 rows
/// keep each step near 20 ms while the loading screen yields between steps.
pub const SOIL_ROWS_PER_STEP: usize = 128;

/// An in-progress banded soil bake.
#[derive(Default)]
pub struct SoilBake {
    rows: usize,
    pixels: Vec<u8>,
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

/// Every generated texture key a scenery tree samples.
pub fn generated_keys(root: &Node) -> Vec<&'static str> {
    let mut keys = Vec::new();
    let mut add = |texture: &Option<TextureRef>| {
        if let Some(TextureRef {
            source: TextureSource::Generated(key),
            ..
        }) = texture
            && !keys.contains(key)
        {
            keys.push(*key);
        }
    };
    root.traverse(glam::DMat4::IDENTITY, &mut |node, _| {
        if let Some(drawable) = &node.drawable {
            let material = &drawable.material;
            add(&material.map);
            add(&material.emissive_map);
            add(&material.bump_map);
            for (_, texture) in &material.extra_textures {
                add(&Some(texture.clone()));
            }
        }
    });
    keys
}

/// Generated textures presentation has supplied or is baking.
#[derive(Default)]
pub struct GeneratedTextures {
    done: HashSet<&'static str>,
    soil: Option<SoilBake>,
}

impl GeneratedTextures {
    /// Whether a band of work remains (the soil bake).
    pub fn busy(&self) -> bool {
        self.soil.is_some()
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use crate::gpu::Renderer;
    use sloppy_core::models::effects_scenery::{CanvasOp, CanvasTexture};
    use wasm_bindgen::JsCast;

    impl GeneratedTextures {
        /// Supply the canvas textures a scenery tree uses now, and start baking
        /// its soil (finished by [`Self::step`]).
        pub fn request_scenery(&mut self, renderer: &mut Renderer, root: &Node) {
            for key in generated_keys(root) {
                if !self.done.insert(key) {
                    continue;
                }
                if key == QUARRY_SOIL_TEXTURE {
                    self.soil = Some(SoilBake::default());
                } else if let Some(canvas) = canvas_texture(key) {
                    match draw_canvas(&canvas) {
                        Ok(rgba) => {
                            renderer.set_generated_texture(key, canvas.width, canvas.height, rgba)
                        }
                        Err(error) => web_sys::console::error_1(
                            &format!("canvas texture {key}: {error:?}").into(),
                        ),
                    }
                }
            }
        }

        /// Bake one band of pending soil; uploads it once complete.
        pub fn step(&mut self, renderer: &mut Renderer, rows: usize) {
            if let Some(bake) = &mut self.soil
                && let Some(pixels) = bake.step(rows)
            {
                let size = QUARRY_SOIL_SIZE as u32;
                renderer.set_generated_texture(QUARRY_SOIL_TEXTURE, size, size, pixels);
                self.soil = None;
            }
        }
    }

    /// Replay the Canvas 2D steps on a fresh, transparent `OffscreenCanvas` and
    /// read back its RGBA rows (top first, straight alpha).
    fn draw_canvas(canvas: &CanvasTexture) -> Result<Vec<u8>, wasm_bindgen::JsValue> {
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
        let image = context.get_image_data(0.0, 0.0, canvas.width as f64, canvas.height as f64)?;
        Ok(image.data().0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::models::quarry_soil_pixels;

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
