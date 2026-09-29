//! The WebGPU renderer. It uploads `sloppy_core::scene` models, draws them with
//! handwritten WGSL, and never moves simulation state.
//!
//! Pure CPU parts (color management, cameras and picking, model preparation and
//! batching, draw-list building, shader assembly) compile natively and carry the
//! unit tests. The `gpu` module is browser-only (`wgpu` on WebGPU).

pub mod camera;
pub mod color;
pub mod draw_list;
pub mod effects;
pub mod material;
pub mod model;
pub mod presentation;
pub mod shader;
pub mod shadow_merge;

#[cfg(target_arch = "wasm32")]
pub mod gpu;

pub use camera::{Frustum, PerspectiveCamera, Ray, ShadowCamera, Sphere};
pub use effects::{EffectDefinition, EffectRegistry};
#[cfg(target_arch = "wasm32")]
pub use gpu::{
    Environment, Fog, InstanceId, Lifetime, ModelId, PointLight, PoolId, PrepareProgress,
    RenderStats, Renderer, RendererOptions, SunShadow, WaterSettings, WaterShore,
};
