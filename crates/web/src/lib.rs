//! wasm-bindgen exports for the page: a thin, coarse-grained API over the shared core
//! and the renderer. Hot loops and game state stay on this side of the boundary.
//!
//! - [`Game`] (`game.rs`): single-player play, one call per frame.
//! - [`NetGame`] (`net_game.rs`): one multiplayer room page, one call per frame.
//! - `RenderLab` (`lab.rs`) and `EffectsLab` (`effects_lab.rs`): the development labs of
//!   `tools/render-lab.html` and `tools/effects-lab.html`, built only with the `labs` feature.

#![recursion_limit = "256"]

pub mod hud;
pub mod lab_scene;

#[cfg(all(target_arch = "wasm32", feature = "labs"))]
mod effects_lab;
#[cfg(target_arch = "wasm32")]
mod game;
#[cfg(all(target_arch = "wasm32", feature = "labs"))]
mod lab;
#[cfg(target_arch = "wasm32")]
mod net_game;
#[cfg(target_arch = "wasm32")]
mod stats;

#[cfg(all(target_arch = "wasm32", feature = "labs"))]
pub use effects_lab::EffectsLab;
#[cfg(target_arch = "wasm32")]
pub use game::Game;
#[cfg(all(target_arch = "wasm32", feature = "labs"))]
pub use lab::RenderLab;
#[cfg(target_arch = "wasm32")]
pub use net_game::NetGame;

/// Rows `band` of `bands` (equal shares, top first) of a generated texture key as
/// RGBA8, for the page's bake workers: further instances of this module that run only
/// this call, off the main thread (`Game::claim_texture_bake`). `None` for a key
/// the engine does not bake.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn bake_texture(key: &str, band: u32, bands: u32) -> Option<js_sys::Uint8Array> {
    use sloppy_core::models::effects_scenery::QUARRY_SOIL_TEXTURE;
    use sloppy_core::models::{QUARRY_SOIL_SIZE, bake_quarry_soil, sand_accum};
    if key != QUARRY_SOIL_TEXTURE || band >= bands {
        return None;
    }
    let row = |band: u32| QUARRY_SOIL_SIZE * band as usize / bands as usize;
    let pixels = bake_quarry_soil(sand_accum(), row(band), row(band + 1));
    Some(js_sys::Uint8Array::from(&pixels[..]))
}
