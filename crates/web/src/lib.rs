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
