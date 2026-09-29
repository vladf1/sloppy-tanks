//! wasm-bindgen exports for the page: a thin, coarse-grained API over the shared core
//! and the renderer. Hot loops and game state stay on this side of the boundary.
//!
//! For now this hosts the render lab (`tools/render-lab.html`), which draws a
//! calibration scene described in JSON; the game API joins it later.

pub mod lab_scene;

#[cfg(target_arch = "wasm32")]
mod effects_lab;
#[cfg(target_arch = "wasm32")]
mod lab;

#[cfg(target_arch = "wasm32")]
pub use effects_lab::EffectsLab;
#[cfg(target_arch = "wasm32")]
pub use lab::RenderLab;
