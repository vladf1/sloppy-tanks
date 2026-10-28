//! Background pipeline compilation.
//!
//! wgpu creates render pipelines with the browser's synchronous
//! `GPUDevice.createRenderPipeline`, which Chrome compiles on the GPU process's main
//! thread. With a cold system shader cache an arena's pipelines take seconds there,
//! and the whole browser stalls meanwhile: no animation frames or timers in this or
//! other tabs. `createRenderPipelineAsync` compiles on background threads instead.
//!
//! So every pipeline is first described once as a [`PipelineSpec`]. The precompiler
//! issues `createRenderPipelineAsync` for the browser descriptor that wgpu's WebGPU
//! backend would build from it (same WGSL, bind group layouts, vertex buffers, states
//! and targets); once that resolves, wgpu wraps that very pipeline (`browser::hand_over`).
//! Letting wgpu create its twin cost WebKit the whole job again, cache hits and all
//! (2-9 ms a pipeline, warm or cold, on the GPU process's one queue: about 130 ms of
//! a harbor start). The mapping below mirrors wgpu 30's `backend/webgpu.rs` for the
//! formats and states the game's pipelines use; anything else is reported and
//! compiles synchronously. The precompiler's bind group layouts are its own copies,
//! group-equivalent to wgpu's, so wgpu's bind groups draw with its pipelines.
//!
//! The WebGL backend has its own background compile (`webgl/programs.rs`).
//!
//! Natively (Metal, Direct3D 12) wgpu compiles each pipeline on the spot while
//! the arena prepares behind the loading screen, so the precompiler there only keeps
//! the same interface ([`native`]).

use crate::shader::StageSources;

#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(not(target_arch = "wasm32"))]
mod native;

#[cfg(target_arch = "wasm32")]
pub use browser::{Background, Compiled, Precompiler};
#[cfg(not(target_arch = "wasm32"))]
pub use native::{Background, Compiled, Precompiler};

/// The bind groups a pipeline layout is made of, shared by wgpu's layouts and the
/// precompiler's copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutKind {
    /// Frame and material groups: surface, shadow and alpha-tested caster pipelines.
    Surface,
    Water,
    Output,
    Bloom,
    Occlusion,
    /// Frame group only: depth-only merged shadow casters and the sky.
    ShadowMerged,
    Mipmap,
    /// No groups: texture bakes, which work out every texel from its uv alone.
    Bake,
}

impl LayoutKind {
    /// The browser precompiler's copies of the layout's groups.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn groups(self) -> &'static [&'static [wgpu::BindGroupLayoutEntry]] {
        use super::resources::{
            BLOOM_ENTRIES, FRAME_ENTRIES, MATERIAL_ENTRIES, MIPMAP_SOURCE_ENTRIES,
            OCCLUSION_ENTRIES, OUTPUT_ENTRIES, TEXTURED_ENTRIES,
        };
        match self {
            LayoutKind::Surface => &[FRAME_ENTRIES, MATERIAL_ENTRIES],
            LayoutKind::Water => &[FRAME_ENTRIES, TEXTURED_ENTRIES],
            LayoutKind::Output => &[OUTPUT_ENTRIES],
            LayoutKind::Bloom => &[BLOOM_ENTRIES],
            LayoutKind::Occlusion => &[OCCLUSION_ENTRIES],
            LayoutKind::ShadowMerged => &[FRAME_ENTRIES],
            LayoutKind::Mipmap => &[MIPMAP_SOURCE_ENTRIES],
            LayoutKind::Bake => &[],
        }
    }
}

/// Everything a render pipeline is made from except its shader code, whose
/// [`StageSources`] make both wgpu's modules and the precompiler's: a module per
/// stage, each with only that stage's code.
#[derive(Debug)]
pub struct PipelineSpec {
    pub label: &'static str,
    pub layout: LayoutKind,
    pub vertex_entry: &'static str,
    pub fragment_entry: Option<&'static str>,
    pub buffers: Vec<wgpu::VertexBufferLayout<'static>>,
    pub targets: Vec<Option<wgpu::ColorTargetState>>,
    pub primitive: wgpu::PrimitiveState,
    pub depth_stencil: Option<wgpu::DepthStencilState>,
    pub multisample: wgpu::MultisampleState,
}

impl PipelineSpec {
    /// The stages of the module `code` that this pipeline runs.
    pub fn stages(&self, code: &str) -> StageSources {
        StageSources::of(code, self.vertex_entry, self.fragment_entry)
    }

    /// The wgpu pipeline: the one [`Precompiler`] compiled from this spec if
    /// `compiled` holds it, else created now with modules of its own (`pipelines.rs`
    /// says why).
    pub fn create(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        sources: &StageSources,
        compiled: Option<&Compiled>,
    ) -> wgpu::RenderPipeline {
        #[cfg(target_arch = "wasm32")]
        if let Some(pipeline) = compiled.filter(|pipeline| !pipeline.is_null()) {
            return browser::hand_over(device, pipeline, || {
                self.create_new(device, layout, sources)
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = compiled;
        self.create_new(device, layout, sources)
    }

    fn create_new(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        sources: &StageSources,
    ) -> wgpu::RenderPipeline {
        let module = |stage, source: &str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("{} {stage}", self.label)),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            })
        };
        let vertex = module("vertex", &sources.vertex);
        let fragment = sources
            .fragment
            .as_deref()
            .map(|source| module("fragment", source));
        let buffers: Vec<_> = self.buffers.iter().cloned().map(Some).collect();
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(self.label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &vertex,
                entry_point: Some(self.vertex_entry),
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            primitive: self.primitive,
            depth_stencil: self.depth_stencil.clone(),
            multisample: self.multisample,
            fragment: self.fragment_entry.map(|entry| wgpu::FragmentState {
                module: fragment
                    .as_ref()
                    .expect("a fragment entry point has a fragment source"),
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &self.targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    }
}
