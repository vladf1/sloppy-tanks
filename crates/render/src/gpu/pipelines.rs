//! Render pipelines, cached by `PipelineKey` for the page's lifetime (the game's
//! variants are bounded, and recompiling them after every round reset would stall;
//! see `keepReleasedPipelines` in the former renderer.ts). Shader sources are
//! cached by `ShaderKey`.
//!
//! Every pipeline gets shader module objects of its own, even when it shares the
//! source with others. WebKit fully recompiles a pipeline whose module object an
//! earlier pipeline used (150-180 ms for a lit surface on a cold cache, one pipeline
//! at a time), while a new module whose source it has compiled before costs about
//! 2 ms whatever the pipeline state. Chrome looks modules up by their source, so the
//! extra objects cost it nothing.
//!
//! Every pipeline is compiled in the background first (`precompile.rs`): the fixed
//! ones while the renderer is created, the scene's variants through
//! [`Pipelines::request`] while an arena prepares. Only a variant first met while
//! drawing ([`Pipelines::ensure`]) still compiles synchronously.

use std::collections::HashMap;

use sloppy_core::scene::Side;

use crate::effects::EffectRegistry;
use crate::gpu::context::{DEPTH_FORMAT, HDR_FORMAT};
use crate::gpu::precompile::{LayoutKind, PipelineSpec, Precompiler, RawModule};
use crate::gpu::resources::Layouts;
use crate::model::Vertex;
use crate::shader::{
    BlendMode, Pass, PipelineKey, ShaderKey, shader_source, shadow_cutout_source,
    shadow_merged_source, water_source,
};
use crate::shadow_merge::ShadowVertex;

pub const SAMPLE_COUNT: u32 = 4;

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x3,
    2 => Float32x2,
    3 => Float32x4,
];
static EXTRA_ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
    4 => Float32x4,
    5 => Float32x4,
];
const SHADOW_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Uint32,
    3 => Float32x2,
];
const SHADOW_BASE_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![2 => Uint32];

/// Merged shadow-caster pipelines by drawn side (see `shadow_merge.rs`).
pub const SHADOW_MERGED_SIDES: [Side; 3] = [Side::Front, Side::Back, Side::Double];

/// The merged shadow pipeline for a drawn side; cutout pipelines follow the
/// depth-only ones.
pub fn shadow_merged_index(side: Side, cutout: bool) -> usize {
    let side = match side {
        Side::Front => 0,
        Side::Back => 1,
        Side::Double => 2,
    };
    side + if cutout { SHADOW_MERGED_SIDES.len() } else { 0 }
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &VERTEX_ATTRIBUTES,
    }
}

fn cull_mode(side: Side) -> Option<wgpu::Face> {
    match side {
        Side::Front => Some(wgpu::Face::Back),
        Side::Back => Some(wgpu::Face::Front),
        Side::Double => None,
    }
}

fn blend_state(mode: BlendMode) -> Option<wgpu::BlendState> {
    use wgpu::{BlendComponent, BlendFactor, BlendOperation};
    let component = |src_factor, dst_factor| BlendComponent {
        src_factor,
        dst_factor,
        operation: BlendOperation::Add,
    };
    match mode {
        BlendMode::Replace => None,
        BlendMode::Normal => Some(wgpu::BlendState {
            color: component(BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha),
            alpha: component(BlendFactor::One, BlendFactor::OneMinusSrcAlpha),
        }),
        BlendMode::Additive => Some(wgpu::BlendState {
            color: component(BlendFactor::SrcAlpha, BlendFactor::One),
            alpha: component(BlendFactor::One, BlendFactor::One),
        }),
    }
}

fn depth_state(
    write: bool,
    compare: wgpu::CompareFunction,
    bias: wgpu::DepthBiasState,
) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(write),
        depth_compare: Some(compare),
        stencil: Default::default(),
        bias,
    }
}

fn water_spec() -> PipelineSpec {
    PipelineSpec {
        label: "water",
        layout: LayoutKind::Water,
        vertex_entry: "vs_water",
        fragment_entry: Some("fs_water"),
        buffers: vec![vertex_layout()],
        targets: vec![Some(HDR_FORMAT.into())],
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(depth_state(
            true,
            wgpu::CompareFunction::LessEqual,
            Default::default(),
        )),
        multisample: wgpu::MultisampleState {
            count: SAMPLE_COUNT,
            ..Default::default()
        },
    }
}

fn output_spec(canvas_format: wgpu::TextureFormat) -> PipelineSpec {
    PipelineSpec {
        label: "output",
        layout: LayoutKind::Output,
        vertex_entry: "vs_fullscreen",
        fragment_entry: Some("fs_output"),
        buffers: vec![],
        targets: vec![Some(canvas_format.into())],
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
    }
}

fn shadow_merged_spec(side: Side, cutout: bool) -> PipelineSpec {
    PipelineSpec {
        label: if cutout {
            "shadow cutout"
        } else {
            "shadow merged"
        },
        layout: if cutout {
            LayoutKind::Surface
        } else {
            LayoutKind::ShadowMerged
        },
        vertex_entry: if cutout {
            "vs_shadow_cutout"
        } else {
            "vs_shadow_merged"
        },
        fragment_entry: cutout.then_some("fs_shadow_cutout"),
        buffers: vec![
            wgpu::VertexBufferLayout {
                array_stride: size_of::<ShadowVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &SHADOW_VERTEX_ATTRIBUTES,
            },
            wgpu::VertexBufferLayout {
                array_stride: 4,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &SHADOW_BASE_ATTRIBUTES,
            },
        ],
        targets: vec![],
        primitive: wgpu::PrimitiveState {
            cull_mode: cull_mode(side),
            ..Default::default()
        },
        depth_stencil: Some(depth_state(
            true,
            wgpu::CompareFunction::LessEqual,
            Default::default(),
        )),
        multisample: Default::default(),
    }
}

/// A surface or shadow variant.
fn surface_spec(key: &PipelineKey, effects: &EffectRegistry) -> PipelineSpec {
    let shader = key.shader;
    let main = shader.pass == Pass::Main;
    let mut buffers = vec![vertex_layout()];
    if shader.extra_attributes > 0 {
        buffers.push(wgpu::VertexBufferLayout {
            array_stride: 16 * shader.extra_attributes as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &EXTRA_ATTRIBUTES[..shader.extra_attributes as usize],
        });
    }
    let fragment_entry = if main {
        Some("fs_main")
    } else if shader.shadow_needs_fragment(effects) {
        Some("fs_shadow")
    } else {
        None
    };
    let targets = if main {
        vec![Some(wgpu::ColorTargetState {
            format: HDR_FORMAT,
            blend: blend_state(key.blend),
            write_mask: wgpu::ColorWrites::ALL,
        })]
    } else {
        vec![]
    };
    PipelineSpec {
        label: if main { "surface" } else { "shadow" },
        layout: LayoutKind::Surface,
        vertex_entry: if main { "vs_main" } else { "vs_shadow" },
        fragment_entry,
        buffers,
        targets,
        primitive: wgpu::PrimitiveState {
            cull_mode: cull_mode(key.side),
            ..Default::default()
        },
        depth_stencil: Some(depth_state(
            key.depth_write,
            if key.depth_test {
                wgpu::CompareFunction::LessEqual
            } else {
                wgpu::CompareFunction::Always
            },
            wgpu::DepthBiasState {
                constant: key.depth_bias.constant,
                slope_scale: key.depth_bias.slope_scale(),
                clamp: 0.0,
            },
        )),
        multisample: wgpu::MultisampleState {
            count: if main { SAMPLE_COUNT } else { 1 },
            mask: !0,
            alpha_to_coverage_enabled: key.alpha_to_coverage,
        },
    }
}

/// The cached WGSL of a surface or shadow variant.
fn source<'a>(
    sources: &'a mut HashMap<ShaderKey, String>,
    effects: &EffectRegistry,
    shader: ShaderKey,
) -> &'a str {
    sources
        .entry(shader)
        .or_insert_with(|| shader_source(&shader, effects))
}

/// A wgpu shader module for one pipeline (see the module comment).
fn module(device: &wgpu::Device, label: &str, source: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

fn pipeline_layout(
    device: &wgpu::Device,
    label: &str,
    groups: &[&wgpu::BindGroupLayout],
) -> wgpu::PipelineLayout {
    let groups: Vec<_> = groups.iter().copied().map(Some).collect();
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &groups,
        immediate_size: 0,
    })
}

pub struct Pipelines {
    precompiler: Precompiler<PipelineKey>,
    sources: HashMap<ShaderKey, String>,
    pipelines: Vec<wgpu::RenderPipeline>,
    index: HashMap<PipelineKey, u32>,
    surface_layout: wgpu::PipelineLayout,
    pub water: wgpu::RenderPipeline,
    pub output: wgpu::RenderPipeline,
    /// Merged casters, depth-only then alpha-tested, indexed by
    /// [`shadow_merged_index`].
    pub shadow_merged: Vec<wgpu::RenderPipeline>,
}

impl Pipelines {
    /// Compile the fixed water, output and merged shadow pipelines in the background,
    /// then create them.
    pub async fn new(
        device: &wgpu::Device,
        layouts: &Layouts,
        canvas_format: wgpu::TextureFormat,
    ) -> Self {
        let precompiler = Precompiler::new(device);
        let surface_layout =
            pipeline_layout(device, "surface", &[&layouts.frame, &layouts.material]);
        let water_layout = pipeline_layout(device, "water", &[&layouts.frame, &layouts.water]);
        let output_layout = pipeline_layout(device, "output", &[&layouts.output]);
        let merged_layout = pipeline_layout(device, "shadow merged", &[&layouts.frame]);
        let water_source = water_source();
        let merged_source = shadow_merged_source();
        let cutout_source = shadow_cutout_source();
        let water = (water_spec(), &water_layout, water_source.as_str());
        let output = (
            output_spec(canvas_format),
            &output_layout,
            crate::shader::OUTPUT_WGSL,
        );
        let merged: Vec<_> = [false, true]
            .into_iter()
            .flat_map(|cutout| SHADOW_MERGED_SIDES.iter().map(move |&side| (cutout, side)))
            .map(|(cutout, side)| {
                let spec = shadow_merged_spec(side, cutout);
                if cutout {
                    (spec, &surface_layout, cutout_source.as_str())
                } else {
                    (spec, &merged_layout, merged_source.as_str())
                }
            })
            .collect();
        let fixed: Vec<_> = [&water, &output].into_iter().chain(&merged).collect();
        let raw: Vec<RawModule> = fixed
            .iter()
            .map(|(spec, _, source)| precompiler.module(spec.label, source))
            .collect();
        let jobs: Vec<_> = fixed
            .iter()
            .zip(&raw)
            .map(|(job, raw)| (&job.0, raw))
            .collect();
        let _compiled = precompiler.compile_all(&jobs).await;
        let create = |(spec, layout, source): &(PipelineSpec, &wgpu::PipelineLayout, &str)| {
            spec.create(device, layout, &module(device, spec.label, source))
        };
        Self {
            water: create(&water),
            output: create(&output),
            shadow_merged: merged.iter().map(create).collect(),
            precompiler,
            sources: HashMap::new(),
            pipelines: Vec::new(),
            index: HashMap::new(),
            surface_layout,
        }
    }

    pub fn find(&self, key: &PipelineKey) -> Option<u32> {
        self.index.get(key).copied()
    }

    pub fn get(&self, index: u32) -> &wgpu::RenderPipeline {
        &self.pipelines[index as usize]
    }

    /// The pipeline for a key once the background compile has finished, created
    /// then only if `create` (the caller's per-step budget); until then it queues the
    /// compile and returns `None`.
    pub fn request(
        &mut self,
        device: &wgpu::Device,
        effects: &EffectRegistry,
        key: &PipelineKey,
        create: bool,
    ) -> Option<u32> {
        if let Some(index) = self.find(key) {
            return Some(index);
        }
        if self.precompiler.finished(key) {
            return create.then(|| self.create(device, effects, key));
        }
        if !self.precompiler.queued(key) {
            let spec = surface_spec(key, effects);
            let source = source(&mut self.sources, effects, key.shader);
            let raw = self.precompiler.module(spec.label, source);
            self.precompiler.start(*key, &spec, &raw);
        }
        None
    }

    /// The pipeline for a key, compiling it synchronously on first use (a stall on a
    /// cold shader cache; preparation uses [`Self::request`]).
    pub fn ensure(
        &mut self,
        device: &wgpu::Device,
        effects: &EffectRegistry,
        key: &PipelineKey,
    ) -> u32 {
        match self.find(key) {
            Some(index) => index,
            None => self.create(device, effects, key),
        }
    }

    fn create(
        &mut self,
        device: &wgpu::Device,
        effects: &EffectRegistry,
        key: &PipelineKey,
    ) -> u32 {
        let spec = surface_spec(key, effects);
        let module = module(
            device,
            spec.label,
            source(&mut self.sources, effects, key.shader),
        );
        let pipeline = spec.create(device, &self.surface_layout, &module);
        self.precompiler.release(key);
        self.pipelines.push(pipeline);
        let index = self.pipelines.len() as u32 - 1;
        self.index.insert(*key, index);
        index
    }

    /// Variants queued for or in background compilation.
    pub fn compiling(&self) -> u32 {
        self.precompiler.compiling()
    }

    /// Pipelines including the fixed water, output and merged shadow ones.
    pub fn count(&self) -> usize {
        self.pipelines.len() + 2 + self.shadow_merged.len()
    }

    /// Distinct shader sources, including the fixed water, output and merged shadow
    /// ones.
    pub fn module_count(&self) -> usize {
        self.sources.len() + 4
    }
}
