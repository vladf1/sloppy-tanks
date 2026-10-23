//! The renderer. It uploads `sloppy_core::scene` models, draws them with
//! handwritten WGSL, and never moves simulation state.
//!
//! Pure CPU parts (color management, cameras and picking, model preparation and
//! batching, mesh page placement, draw-list building, shader assembly, render
//! target sizes) compile
//! natively and carry the unit tests. The `gpu` module is browser-only: `wgpu` on
//! WebGPU, or with the `webgl` feature glow on WebGL2.

/// The WGSL file at each path under `src/`, one after another, embedded without
/// comments or blank lines (`build.rs`): the shader text the binary carries.
macro_rules! wgsl {
    ($($path:literal),+ $(,)?) => {
        concat!($(include_str!(concat!(env!("OUT_DIR"), "/wgsl/", $path))),+)
    };
}

pub mod atmosphere_map;
pub mod camera;
pub mod color;
pub mod draw_list;
pub mod effects;
pub mod material;
pub mod mesh_pages;
pub mod model;
pub mod noise_lattice;
pub mod presentation;
pub mod quality;
pub mod reflection_cull;
pub mod shader;
pub mod shadow_merge;
pub mod target_memory;

#[cfg(target_arch = "wasm32")]
pub mod gpu;

#[cfg(all(
    target_arch = "wasm32",
    not(any(feature = "webgpu", feature = "webgl"))
))]
compile_error!("the browser renderer needs the `webgpu` or the `webgl` feature");

#[cfg(target_arch = "wasm32")]
pub use gpu::GRAPHICS_API;
