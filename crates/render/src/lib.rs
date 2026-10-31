//! The renderer. It uploads `sloppy_core::scene` models, draws them with
//! handwritten WGSL, and never moves simulation state.
//!
//! Pure CPU parts (color management, cameras and picking, model preparation and
//! batching, mesh page placement, draw-list building, shader assembly, render
//! target sizes) compile
//! natively and carry the unit tests. The `gpu` module draws with `wgpu` on the
//! browser's WebGPU, with the `webgl` feature glow on WebGL2, or with the `native`
//! feature `wgpu` on Metal or Direct3D 12 (the desktop builds).

/// The WGSL file at each path under `src/`, one after another, embedded without
/// comments or blank lines (`build.rs`): the shader text the binary carries.
#[cfg(any(test, feature = "specializer"))]
macro_rules! wgsl {
    ($($path:literal),+ $(,)?) => {
        concat!($(include_str!(concat!(env!("OUT_DIR"), "/wgsl/", $path))),+)
    };
}

/// Without the specializer, only the lines of those files that declare an effect hook
/// (`build.rs`): the binary draws the generated variants (`shader/tables.rs`).
#[cfg(not(any(test, feature = "specializer")))]
macro_rules! wgsl {
    ($($path:literal),+ $(,)?) => {
        concat!($(include_str!(concat!(env!("OUT_DIR"), "/wgsl-hooks/", $path))),+)
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

#[cfg(any(target_arch = "wasm32", feature = "native"))]
pub mod gpu;
/// Natively, and for the desktop shell in a page, where it finds none (a page cannot
/// read the system's font files).
#[cfg(any(feature = "native", target_arch = "wasm32"))]
pub mod system_fonts;

/// The wgpu the renderer draws with, for the desktop shell's overlay (natively and in
/// the browser).
#[cfg(any(
    all(not(target_arch = "wasm32"), feature = "native"),
    all(target_arch = "wasm32", not(feature = "webgl"))
))]
pub use wgpu;

#[cfg(all(
    target_arch = "wasm32",
    not(any(feature = "webgpu", feature = "webgl"))
))]
compile_error!("the browser renderer needs the `webgpu` or the `webgl` feature");

#[cfg(any(target_arch = "wasm32", feature = "native"))]
pub use gpu::GRAPHICS_API;
