//! wasm-bindgen exports for the page: a thin, coarse-grained API over the shared core
//! and the renderer. Hot loops and game state stay on this side of the boundary.
//! With the `native` feature the same `Game` and `NetGame` serve the desktop shell
//! (`crates/desktop`) as plain Rust.
//!
//! - [`Game`] (`game.rs`): single-player play, one call per frame.
//! - [`NetGame`] (`net_game.rs`): one multiplayer room page, one call per frame.
//! - [`WireView`]: binary room messages as the former JSON, for the Node checks.
//! - `RenderLab` (`lab.rs`) and `EffectsLab` (`effects_lab.rs`): the development labs of
//!   `tools/render-lab.html` and `tools/effects-lab.html`, built only with the `labs` feature.

#![recursion_limit = "256"]

pub mod events;
pub mod frame_clock;
pub mod hud;
pub mod lab_scene;

#[cfg(all(target_arch = "wasm32", feature = "labs"))]
mod effects_lab;
#[cfg(any(target_arch = "wasm32", feature = "native"))]
mod game;
#[cfg(all(target_arch = "wasm32", feature = "labs"))]
mod lab;
#[cfg(any(target_arch = "wasm32", feature = "native"))]
mod net_game;
#[cfg(any(target_arch = "wasm32", feature = "native"))]
mod page;
#[cfg(all(not(target_arch = "wasm32"), feature = "native"))]
mod records;
#[cfg(any(target_arch = "wasm32", feature = "native"))]
mod stats;

#[cfg(all(target_arch = "wasm32", feature = "labs"))]
pub use effects_lab::EffectsLab;
#[cfg(any(target_arch = "wasm32", feature = "native"))]
pub use game::{Game, frame_slot};
#[cfg(all(target_arch = "wasm32", feature = "labs"))]
pub use lab::RenderLab;
#[cfg(any(target_arch = "wasm32", feature = "native"))]
pub use net_game::{NetGame, net_frame_slot};
#[cfg(all(not(target_arch = "wasm32"), feature = "native"))]
pub use page::{PageError, clock_start};
#[cfg(all(not(target_arch = "wasm32"), feature = "native"))]
pub use records::set_records_file;

/// One connection's binary room messages as the JSON protocol sent them (see
/// `sloppy_core::net::wire_view`), for the Node checks that follow a room's state
/// (`scripts/state-mirror.mjs`). The page itself never needs it.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
#[derive(Default)]
pub struct WireView(sloppy_core::net::wire_view::WireView);

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
impl WireView {
    #[wasm_bindgen::prelude::wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// The message as JSON text; frames advance this view's mirror.
    pub fn json(&mut self, bytes: &[u8]) -> Result<String, wasm_bindgen::JsValue> {
        self.0
            .binary(bytes)
            .map(|value| value.to_string())
            .map_err(|error| wasm_bindgen::JsValue::from_str(&error))
    }
}

/// The generated textures the arena of Battle Setup map `map` will have the page bake
/// (`bake_texture` keys). The page may start them as soon as the engine has loaded,
/// before the arena is built and claims them (`Game::claim_texture_bake`).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn map_texture_bakes(map: &str) -> Vec<String> {
    use sloppy_core::sim::map_options::MapId;
    use sloppy_core::sim::maps::MAPS;
    let theme = MapId::parse(map)
        .and_then(|id| MAPS.iter().find(|candidate| candidate.id == id))
        .and_then(|map| map.theme);
    theme.map_or_else(Vec::new, |theme| {
        sloppy_render::presentation::generated::theme_bakes(theme.as_str())
            .iter()
            .map(|key| key.to_string())
            .collect()
    })
}

/// Rows `band` of `bands` (equal shares, top first) of a generated texture key as
/// RGBA8, for the page's bake workers: further instances of this module that run only
/// this call, off the main thread (`Game::claim_texture_bake`). `None` for a key
/// the engine does not bake.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn bake_texture(key: &str, band: u32, bands: u32) -> Option<js_sys::Uint8Array> {
    use sloppy_core::models::effects_scenery::QUARRY_SOIL_TEXTURE;
    use sloppy_core::models::{QUARRY_SOIL_SIZE, bake_quarry_ground, sand_accum};
    if key != QUARRY_SOIL_TEXTURE || band >= bands {
        return None;
    }
    let row = |band: u32| QUARRY_SOIL_SIZE * band as usize / bands as usize;
    let pixels = bake_quarry_ground(sand_accum(), row(band), row(band + 1));
    Some(js_sys::Uint8Array::from(&pixels[..]))
}
