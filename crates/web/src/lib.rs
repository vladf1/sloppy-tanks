//! wasm-bindgen exports for the page: a thin, coarse-grained API over the shared core
//! and the renderer. Hot loops and game state stay on this side of the boundary.
//!
//! - [`Game`] (`game.rs`): single-player play, one call per frame.
//! - [`RenderLab`] (`lab.rs`): the renderer calibration scene of `tools/render-lab.html`.

#![recursion_limit = "256"]

pub mod lab_scene;

#[cfg(target_arch = "wasm32")]
mod effects_lab;
#[cfg(target_arch = "wasm32")]
mod game;
#[cfg(target_arch = "wasm32")]
mod lab;

#[cfg(target_arch = "wasm32")]
pub use effects_lab::EffectsLab;
#[cfg(target_arch = "wasm32")]
pub use game::Game;
#[cfg(target_arch = "wasm32")]
pub use lab::RenderLab;
