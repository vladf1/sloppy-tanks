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
//! Every pipeline is compiled in the background first (`precompile.rs`), and wgpu
//! wraps the compiled pipeline: the fixed ones while the renderer is created, the
//! scene's variants through [`Pipelines::request`] while an arena prepares. Only a
//! variant first met while drawing ([`Pipelines::ensure`]) still compiles
//! synchronously.

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
    shadow_cutout_stages, shadow_merged_source, sky_source, water_source,
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
/// onto its target: one fragment stage that the instance index tells which pass it
/// draws (`BloomPass`), so the prefilter and downsample are the same pipeline state.
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
        ("bloom prefilter", format.into()),
        ("bloom down", format.into()),
        ("bloom up", add),
    ]
    .map(|(label, target)| post_spec(label, LayoutKind::Bloom, "vs_post", "fs_bloom", target))
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

/// The occlusion's blur for the output pass: the bloom's fragment stage, drawn as
/// `BloomPass::OcclusionBlur`, into the occlusion's format.
fn occlusion_blur_spec() -> PipelineSpec {
    post_spec(
        "ambient occlusion blur",
        LayoutKind::Bloom,
        "vs_post",
        "fs_bloom",
        OCCLUSION_FORMAT.into(),
    )
}

/// The dialog backdrop's blur passes: the bloom's fragment stage, drawn as
/// `BloomPass::BackdropRows` and `BackdropColumns`, into the canvas's format.
fn backdrop_spec(canvas_format: wgpu::TextureFormat) -> PipelineSpec {
    post_spec(
        "backdrop blur",
        LayoutKind::Bloom,
        "vs_post",
        "fs_bloom",
        canvas_format.into(),
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
        // The per-part casters' fragment stage (`shadow_cutout_stages`).
        fragment_entry: cutout.then_some("fs_shadow"),
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

/// Pipelines compiling in the background: the fixed ones (output, sky, the three
/// bloom passes, occlusion and its blur, then the merged casters in
/// `shadow_merged_index` order), or the water's (its surface, then its depth).
struct Jobs {
    jobs: Vec<(PipelineSpec, StageSources)>,
    compile: Background,
}

impl Jobs {
    fn start(device: &wgpu::Device, jobs: Vec<(PipelineSpec, StageSources)>) -> Self {
        let specs: Vec<_> = jobs.iter().map(|(spec, stages)| (spec, stages)).collect();
        let compile = Background::start(device, &specs);
        Self { jobs, compile }
    }
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
    fixed_jobs: Option<Jobs>,
    fixed: Option<FixedPipelines<wgpu::RenderPipeline>>,
    /// The water's pipelines while they compile, from the first arena with water on:
    /// a map without any never compiles them (about 0.35 s of a cold start in WebKit).
    water_jobs: Option<Jobs>,
    /// The water surface, and its depth for the occlusion depth pass.
    water: Option<[wgpu::RenderPipeline; 2]>,
}

impl Pipelines {
    /// Start compiling the fixed output, post and merged shadow pipelines in the
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
        let (output, sky) = (output_source(), sky_source());
        let occlusion = occlusion_source(true, OCCLUSION_DOWNSCALE / OCCLUSION_DEPTH_DOWNSCALE);
        let merged = shadow_merged_source();
        let mut specs = vec![
            (output_spec(canvas_format), &output),
            (sky_spec(gpu.samples), &sky),
        ];
        for spec in bloom_specs(gpu.bloom_format) {
            specs.push((spec, &output));
        }
        specs.push((occlusion_spec(), &occlusion));
        specs.push((occlusion_blur_spec(), &output));
        specs.push((backdrop_spec(canvas_format), &output));
        for &side in &SHADOW_MERGED_SIDES {
            specs.push((shadow_merged_spec(side, false), &merged));
        }
        let mut jobs: Vec<_> = specs
            .into_iter()
            .map(|(spec, source)| {
                let stages = spec.stages(source);
                (spec, stages)
            })
            .collect();
        let cutout = shadow_cutout_stages();
        for &side in &SHADOW_MERGED_SIDES {
            jobs.push((shadow_merged_spec(side, true), cutout.clone()));
        }
        let fixed_jobs = Jobs::start(device, jobs);
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
            fixed_jobs: Some(fixed_jobs),
            fixed: None,
            water_jobs: None,
            water: None,
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
        let Some(jobs) = self.fixed_jobs.take() else {
            return;
        };
        let mut created = self.create_jobs(device, &jobs).into_iter();
        let mut next = |what| created.next().expect(what);
        let output = next("output pipeline");
        let sky = next("sky pipeline");
        let bloom = [(); 3].map(|_| next("bloom pipeline"));
        let occlusion = next("occlusion pipeline");
        let occlusion_blur = next("occlusion blur pipeline");
        let backdrop = next("backdrop blur pipeline");
        self.fixed = Some(FixedPipelines {
            output,
            sky,
            bloom,
            occlusion,
            occlusion_blur,
            backdrop,
            shadow_merged: created.collect(),
        });
    }

    /// The wgpu pipelines of `jobs`, wrapping those compiled in the background.
    fn create_jobs(&self, device: &wgpu::Device, jobs: &Jobs) -> Vec<wgpu::RenderPipeline> {
        let Jobs { jobs, compile } = jobs;
        jobs.iter()
            .enumerate()
            .map(|(index, (spec, source))| {
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
                spec.create(device, layout, source, compile.compiled(index).as_ref())
            })
            .collect()
    }

    /// Start compiling the water's pipelines in the background unless they exist or
    /// are compiling: called once an arena has water.
    pub fn start_water(&mut self, gpu: &Gpu) {
        if self.water.is_none() && self.water_jobs.is_none() {
            let water = water_source();
            let jobs = [water_spec(gpu.samples), water_depth_spec()]
                .into_iter()
                .map(|spec| {
                    let stages = spec.stages(&water);
                    (spec, stages)
                })
                .collect();
            self.water_jobs = Some(Jobs::start(&gpu.device, jobs));
        }
    }

    /// Whether the water's pipelines exist, creating them once their background
    /// compile has finished (and starting it if needed).
    pub fn water_ready(&mut self, gpu: &Gpu) -> bool {
        self.start_water(gpu);
        if let Some(jobs) = self.water_jobs.take_if(|jobs| jobs.compile.done()) {
            self.create_water(&gpu.device, &jobs);
        }
        self.water.is_some()
    }

    /// Create the water's pipelines now, compiling them synchronously if their
    /// background compile has not finished; returns whether that stalled.
    pub fn ensure_water(&mut self, gpu: &Gpu) -> bool {
        let stalled = !self.water_ready(gpu);
        if let Some(jobs) = self.water_jobs.take() {
            self.create_water(&gpu.device, &jobs);
        }
        stalled
    }

    fn create_water(&mut self, device: &wgpu::Device, jobs: &Jobs) {
        let [surface, depth] = self
            .create_jobs(device, jobs)
            .try_into()
            .unwrap_or_else(|_| unreachable!("the water's surface and depth pipelines"));
        self.water = Some([surface, depth]);
    }

    /// The fixed pipelines; draws first make sure of them ([`Self::ensure_fixed`]).
    pub fn fixed(&self) -> &FixedPipelines<wgpu::RenderPipeline> {
        self.fixed
            .as_ref()
            .expect("fixed pipelines are created before drawing")
    }

    /// The water surface's pipeline; an arena with water makes sure of it before
    /// drawing ([`Self::ensure_water`]).
    pub fn water(&self) -> &wgpu::RenderPipeline {
        &self
            .water
            .as_ref()
            .expect("water pipelines before water draws")[0]
    }

    /// The water surface's depth-only pipeline, made with [`Self::water`].
    pub fn water_depth(&self) -> &wgpu::RenderPipeline {
        &self
            .water
            .as_ref()
            .expect("water pipelines before water draws")[1]
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
        let compiled = self.precompiler.take(key);
        let pipeline = spec.create(device, &self.surface_layout, sources, compiled.as_ref());
        self.pipelines.push(pipeline);
        let index = self.pipelines.len() as u32 - 1;
        self.index.insert(*key, index);
        index
    }

    /// Variants queued for or in background compilation.
    pub fn compiling(&self) -> u32 {
        self.precompiler.compiling()
    }

    /// Pipelines including the fixed output and merged shadow ones and the water's.
    pub fn count(&self) -> usize {
        self.pipelines.len()
            + self.fixed.as_ref().map_or(0, FixedPipelines::count)
            + self.water.as_ref().map_or(0, |water| water.len())
    }

    /// Distinct shader sources, including the fixed output (with bloom), sky,
    /// occlusion and merged shadow ones and the water's.
    pub fn module_count(&self) -> usize {
        self.sources.len() + 5 + usize::from(self.water.is_some())
    }
}
