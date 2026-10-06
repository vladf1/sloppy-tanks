//! The renderer. It uploads `sloppy_core::scene` models, draws them with
//! handwritten WGSL, and never moves simulation state.
//!
//! Pure CPU parts (color management, cameras and picking, model preparation and
//! batching, mesh page placement, draw-list building, shader assembly, render
//! target sizes) compile
//! natively and carry the unit tests. The `gpu` module is browser-only: `wgpu` on
//! WebGPU, or with the `webgl` feature glow on WebGL2.

pub mod camera;
pub mod color;
pub mod draw_list;
pub mod effects;
pub mod material;
pub mod mesh_pages;
pub mod model;
pub mod presentation;
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

pub use camera::{Frustum, PerspectiveCamera, Ray, ShadowCamera, Sphere};
pub use effects::{EffectDefinition, EffectRegistry};
#[cfg(target_arch = "wasm32")]
pub use gpu::{
    Environment, Fog, GRAPHICS_API, InstanceId, Lifetime, ModelId, PointLight, PoolId,
    PrepareProgress, RenderStats, Renderer, RendererOptions, SunShadow, WaterSettings, WaterShore,
};
