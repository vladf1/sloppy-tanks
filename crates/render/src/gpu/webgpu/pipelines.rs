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
//! Each stage is a module of its own with only that stage's code ([`StageSources`]).
//! WebKit compiles every stage into a Metal library from the module it is given
//! (about 0.1 s each on a cold cache), so the many variants whose vertex stage is the
//! same code share that compile once their fragment code is out of its module: the
//! village's cold compile went from 71 stage compiles to 50.
//!
//! Every pipeline is compiled in the background first (`precompile.rs`): the fixed
//! ones while the renderer is created, the scene's variants through
//! [`Pipelines::request`] while an arena prepares. Only a variant first met while
//! drawing ([`Pipelines::ensure`]) still compiles synchronously.

use std::collections::HashMap;

use sloppy_core::scene::Side;

use super::Gpu;
use super::OCCLUSION_DEPTH_DOWNSCALE;
use super::context::{DEPTH_FORMAT, HDR_FORMAT, OCCLUSION_FORMAT, SHADOW_DEPTH_FORMAT};
use super::precompile::{Background, LayoutKind, PipelineSpec, Precompiler};
use crate::effects::EffectRegistry;
use crate::gpu::{FixedPipelines, OCCLUSION_DOWNSCALE, SHADOW_MERGED_SIDES};
use crate::model::Vertex;
use crate::shader::{
    BlendMode, Pass, PipelineKey, ShaderKey, StageSources, occlusion_source, output_source,
    shadow_cutout_source, shadow_merged_source, sky_source, water_source,
};
use crate::shadow_merge::ShadowVertex;

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x3,
    2 => Float32x2,
    3 => Float32x4,
];
const SHADOW_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Uint32,
    3 => Float32x2,
];
const SHADOW_BASE_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![2 => Uint32];

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
        // The alpha blends like the colour, so it keeps the share of the haze behind
        // that shows through (`haze_alpha`).
        BlendMode::Normal => Some(wgpu::BlendState {
            color: component(BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha),
            alpha: component(BlendFactor::One, BlendFactor::OneMinusSrcAlpha),
        }),
        // Added light hides nothing behind it: the alpha, and so its haze, stays.
        BlendMode::Additive => Some(wgpu::BlendState {
            color: component(BlendFactor::SrcAlpha, BlendFactor::One),
            alpha: component(BlendFactor::Zero, BlendFactor::One),
        }),
    }
}

fn depth_state(
    format: wgpu::TextureFormat,
    write: bool,
    compare: wgpu::CompareFunction,
    bias: wgpu::DepthBiasState,
) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format,
        depth_write_enabled: Some(write),
        depth_compare: Some(compare),
        stencil: Default::default(),
        bias,
    }
}

fn water_spec(samples: u32) -> PipelineSpec {
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
            DEPTH_FORMAT,
            true,
            wgpu::CompareFunction::LessEqual,
            Default::default(),
        )),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
    }
}

/// The water surface's depth alone, for the occlusion depth pass.
fn water_depth_spec() -> PipelineSpec {
    PipelineSpec {
        label: "water depth",
        fragment_entry: None,
        targets: vec![],
        ..water_spec(1)
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

/// The sky: a far-plane triangle drawn where the depth is still clear.
fn sky_spec(samples: u32) -> PipelineSpec {
    PipelineSpec {
        label: "sky",
        layout: LayoutKind::ShadowMerged,
        vertex_entry: "vs_sky",
        fragment_entry: Some("fs_sky"),
        buffers: vec![],
        targets: vec![Some(HDR_FORMAT.into())],
        primitive: Default::default(),
        depth_stencil: Some(depth_state(
            DEPTH_FORMAT,
            false,
            wgpu::CompareFunction::LessEqual,
            Default::default(),
        )),
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
    }
}

/// A full-screen post pass into a single-sample target.
fn post_spec(
    label: &'static str,
    layout: LayoutKind,
    vertex_entry: &'static str,
    fragment_entry: &'static str,
    target: wgpu::ColorTargetState,
) -> PipelineSpec {
    PipelineSpec {
        label,
        layout,
        vertex_entry,
        fragment_entry: Some(fragment_entry),
        buffers: vec![],
        targets: vec![Some(target)],
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
    }
}

/// Bloom prefilter, downsample and upsample into levels of `format`, the last adding
/// onto its target.
fn bloom_specs(format: wgpu::TextureFormat) -> [PipelineSpec; 3] {
    let additive = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    };
    let add = wgpu::ColorTargetState {
        format,
        blend: Some(wgpu::BlendState {
            color: additive,
            alpha: additive,
        }),
        write_mask: wgpu::ColorWrites::ALL,
    };
    [
        ("bloom prefilter", "fs_bloom_prefilter", format.into()),
        ("bloom down", "fs_bloom_down", format.into()),
        ("bloom up", "fs_bloom_up", add),
    ]
    .map(|(label, entry, target)| post_spec(label, LayoutKind::Bloom, "vs_post", entry, target))
}

fn occlusion_spec() -> PipelineSpec {
    post_spec(
        "ambient occlusion",
        LayoutKind::Occlusion,
        "vs_occlusion",
        "fs_occlusion",
        OCCLUSION_FORMAT.into(),
    )
}

fn shadow_merged_spec(side: Side, cutout: bool) -> PipelineSpec {
    let (label, layout, vertex_entry) = if cutout {
        ("shadow cutout", LayoutKind::Surface, "vs_shadow_cutout")
    } else {
        (
            "shadow merged",
            LayoutKind::ShadowMerged,
            "vs_shadow_merged",
        )
    };
    PipelineSpec {
        label,
        layout,
        vertex_entry,
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
            SHADOW_DEPTH_FORMAT,
            true,
            wgpu::CompareFunction::LessEqual,
            Default::default(),
        )),
        multisample: Default::default(),
    }
}

/// A surface or shadow variant for a main view of `samples` MSAA samples.
fn surface_spec(key: &PipelineKey, samples: u32) -> PipelineSpec {
    let main = key.shader.pass == Pass::Main;
    let (vertex_entry, fragment_entry) = key.shader.entry_points();
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
        vertex_entry,
        fragment_entry,
        buffers: vec![vertex_layout()],
        targets,
        primitive: wgpu::PrimitiveState {
            cull_mode: cull_mode(key.side),
            ..Default::default()
        },
        depth_stencil: Some(depth_state(
            if key.shadow_map {
                SHADOW_DEPTH_FORMAT
            } else {
                DEPTH_FORMAT
            },
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
            count: if main { samples } else { 1 },
            mask: !0,
            alpha_to_coverage_enabled: main && samples > 1 && key.shader.alpha_to_coverage,
        },
    }
}

/// The cached stage sources of a surface or shadow variant.
fn source<'a>(
    sources: &'a mut HashMap<ShaderKey, StageSources>,
    effects: &EffectRegistry,
    shader: ShaderKey,
) -> &'a StageSources {
    sources
        .entry(shader)
        .or_insert_with(|| StageSources::variant(&shader, effects))
}

pub(super) fn pipeline_layout(
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

/// The fixed pipelines while they compile in the background.
struct FixedJobs {
    /// Water, output, sky, the three bloom passes, occlusion, the water's depth,
    /// then the merged casters in `shadow_merged_index` order.
    jobs: Vec<(PipelineSpec, StageSources)>,
    compile: Background,
}

pub struct Pipelines {
    precompiler: Precompiler<PipelineKey>,
    sources: HashMap<ShaderKey, StageSources>,
    pipelines: Vec<wgpu::RenderPipeline>,
    index: HashMap<PipelineKey, u32>,
    surface_layout: wgpu::PipelineLayout,
    water_layout: wgpu::PipelineLayout,
    output_layout: wgpu::PipelineLayout,
    bloom_layout: wgpu::PipelineLayout,
    occlusion_layout: wgpu::PipelineLayout,
    merged_layout: wgpu::PipelineLayout,
    fixed_jobs: Option<FixedJobs>,
    fixed: Option<FixedPipelines<wgpu::RenderPipeline>>,
    /// The water surface in the occlusion depth pass, made with the fixed pipelines.
    water_depth: Option<wgpu::RenderPipeline>,
}

impl Pipelines {
    /// Start compiling the fixed water, output and merged shadow pipelines in the
    /// background; they are created once compiled ([`Self::fixed_ready`]) while the
    /// page builds the arena.
    pub fn new(gpu: &Gpu) -> Self {
        let (device, layouts, canvas_format) = (&gpu.device, &*gpu.layouts, gpu.canvas_format);
        let surface_layout =
            pipeline_layout(device, "surface", &[&layouts.frame, &layouts.material]);
        let water_layout = pipeline_layout(device, "water", &[&layouts.frame, &layouts.water]);
        let output_layout = pipeline_layout(device, "output", &[&layouts.output]);
        let bloom_layout = pipeline_layout(device, "bloom", &[&layouts.bloom]);
        let occlusion_layout = pipeline_layout(device, "occlusion", &[&layouts.occlusion]);
        let merged_layout = pipeline_layout(device, "shadow merged", &[&layouts.frame]);
        let (water, output, sky) = (water_source(), output_source(), sky_source());
        let occlusion = occlusion_source(true, OCCLUSION_DOWNSCALE / OCCLUSION_DEPTH_DOWNSCALE);
        let (merged, cutout) = (shadow_merged_source(), shadow_cutout_source());
        let mut specs = vec![
            (water_spec(gpu.samples), &water),
            (output_spec(canvas_format), &output),
            (sky_spec(gpu.samples), &sky),
        ];
        for spec in bloom_specs(gpu.bloom_format) {
            specs.push((spec, &output));
        }
        specs.push((occlusion_spec(), &occlusion));
        specs.push((water_depth_spec(), &water));
        for (is_cutout, source) in [(false, &merged), (true, &cutout)] {
            for &side in &SHADOW_MERGED_SIDES {
                specs.push((shadow_merged_spec(side, is_cutout), source));
            }
        }
        let jobs: Vec<_> = specs
            .into_iter()
            .map(|(spec, source)| {
                let stages = spec.stages(source);
                (spec, stages)
            })
            .collect();
        let specs: Vec<_> = jobs.iter().map(|(spec, stages)| (spec, stages)).collect();
        let compile = Background::start(device, &specs);
        Self {
            precompiler: Precompiler::new(device),
            sources: HashMap::new(),
            pipelines: Vec::new(),
            index: HashMap::new(),
            surface_layout,
            water_layout,
            output_layout,
            bloom_layout,
            occlusion_layout,
            merged_layout,
            fixed_jobs: Some(FixedJobs { jobs, compile }),
            fixed: None,
            water_depth: None,
        }
    }

    /// Whether the fixed pipelines exist, creating them once their background compile
    /// has finished.
    pub fn fixed_ready(&mut self, gpu: &Gpu) -> bool {
        if self
            .fixed_jobs
            .as_ref()
            .is_some_and(|fixed| fixed.compile.done())
        {
            self.create_fixed(&gpu.device);
        }
        self.fixed.is_some()
    }

    /// Create the fixed pipelines now, compiling them synchronously if their
    /// background compile has not finished; returns whether that stalled.
    pub fn ensure_fixed(&mut self, gpu: &Gpu) -> bool {
        let stalled = !self.fixed_ready(gpu);
        if stalled {
            self.create_fixed(&gpu.device);
        }
        stalled
    }

    fn create_fixed(&mut self, device: &wgpu::Device) {
        let Some(FixedJobs {
            jobs,
            compile: _held,
        }) = self.fixed_jobs.take()
        else {
            return;
        };
        let mut created = jobs.iter().map(|(spec, source)| {
            let layout = match spec.layout {
                LayoutKind::Water => &self.water_layout,
                LayoutKind::Output => &self.output_layout,
                LayoutKind::Bloom => &self.bloom_layout,
                LayoutKind::Occlusion => &self.occlusion_layout,
                LayoutKind::ShadowMerged => &self.merged_layout,
                LayoutKind::Surface => &self.surface_layout,
                LayoutKind::Mipmap | LayoutKind::Bake => {
                    unreachable!("the uploader owns the mipmap blits and texture bakes")
                }
            };
            spec.create(device, layout, source)
        });
        let mut next = |what| created.next().expect(what);
        let water = next("water pipeline");
        let output = next("output pipeline");
        let sky = next("sky pipeline");
        let bloom = [(); 3].map(|_| next("bloom pipeline"));
        let occlusion = next("occlusion pipeline");
        self.water_depth = Some(next("water depth pipeline"));
        self.fixed = Some(FixedPipelines {
            water,
            output,
            sky,
            bloom,
            occlusion,
            shadow_merged: created.collect(),
        });
    }

    /// The fixed pipelines; draws first make sure of them ([`Self::ensure_fixed`]).
    pub fn fixed(&self) -> &FixedPipelines<wgpu::RenderPipeline> {
        self.fixed
            .as_ref()
            .expect("fixed pipelines are created before drawing")
    }

    /// The water surface's depth-only pipeline; made with [`Self::fixed`].
    pub fn water_depth(&self) -> &wgpu::RenderPipeline {
        self.water_depth
            .as_ref()
            .expect("fixed pipelines are created before drawing")
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
        gpu: &Gpu,
        effects: &EffectRegistry,
        key: &PipelineKey,
        create: bool,
    ) -> Option<u32> {
        if let Some(index) = self.find(key) {
            return Some(index);
        }
        if self.precompiler.finished(key) {
            return create.then(|| self.create(gpu, effects, key));
        }
        if !self.precompiler.queued(key) {
            let spec = surface_spec(key, gpu.samples);
            let sources = source(&mut self.sources, effects, key.shader);
            self.precompiler.start(*key, &spec, sources);
        }
        None
    }

    /// The pipeline for a key, compiling it synchronously on first use (a stall on a
    /// cold shader cache; preparation uses [`Self::request`]).
    pub fn ensure(&mut self, gpu: &Gpu, effects: &EffectRegistry, key: &PipelineKey) -> u32 {
        match self.find(key) {
            Some(index) => index,
            None => self.create(gpu, effects, key),
        }
    }

    fn create(&mut self, gpu: &Gpu, effects: &EffectRegistry, key: &PipelineKey) -> u32 {
        let device = &gpu.device;
        let spec = surface_spec(key, gpu.samples);
        let sources = source(&mut self.sources, effects, key.shader);
        let pipeline = spec.create(device, &self.surface_layout, sources);
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
        self.pipelines.len()
            + self.fixed.as_ref().map_or(0, FixedPipelines::count)
            + usize::from(self.water_depth.is_some())
    }

    /// Distinct shader sources, including the fixed water, output (with bloom), sky,
    /// occlusion and merged shadow ones.
    pub fn module_count(&self) -> usize {
        self.sources.len() + 6
    }
}
