//! GL programs from the WGSL variants, and pipelines: a program plus the
//! fixed-function state ([`Raster`]) a `PipelineKey` adds. Keys that differ only in
//! that state share one program, so a variant compiles once however many blend,
//! depth or culling setups draw with it. Both are cached for the page's lifetime,
//! like WebGPU's pipelines (`webgpu/pipelines.rs`).
//!
//! With `KHR_parallel_shader_compile` the browser links programs on background
//! threads: [`Pipelines::request`] starts a program and returns it once linked, so
//! preparing an arena never waits on the GPU process. Without it, or for a variant
//! first met while drawing ([`Pipelines::ensure`]), a program links synchronously.

use std::cell::Cell;
use std::collections::HashMap;

use glow::HasContext;
use sloppy_core::scene::Side;

use super::context::{Blend, Cull, Gpu, Raster, block, unit};
use crate::effects::EffectRegistry;
use crate::gpu::SHADOW_MERGED_SIDES;
use crate::shader::glsl::{self, Binding};
use crate::shader::{
    BlendMode, Pass, PipelineKey, ShaderKey, shader_source, shadow_cutout_source,
    shadow_merged_source, water_source,
};

/// The fragment stage of programs whose WGSL has none (depth-only casters).
const EMPTY_FRAGMENT: &str = "#version 300 es\nvoid main(void) {}\n";

/// Stands in for a program whose WGSL failed to translate, after the failure went to
/// the error slot: it draws nothing.
const FAILED_VERTEX: &str = "#version 300 es\nvoid main(void) { gl_Position = vec4(0.0); }\n";

/// The uniform naga adds to `gl_InstanceID` for WGSL's `instance_index`
/// (`naga::back::glsl::FIRST_INSTANCE_BINDING`): WebGL2 has no base instance.
const FIRST_INSTANCE: &str = "naga_vs_first_instance";

/// The uniform block point of a WGSL uniform's binding.
fn block_point(binding: Binding) -> Option<u32> {
    match binding {
        (0, 0) => Some(block::FRAME),
        (1, 0) => Some(block::MATERIAL),
        (0, 1) => Some(block::OUTPUT),
        _ => None,
    }
}

/// The texture unit of a WGSL texture's binding.
fn texture_unit(binding: Binding) -> Option<u32> {
    match binding {
        (0, 0) => Some(unit::SOURCE),
        (0, 1) => Some(unit::SHADOW_MAP),
        (0, 3) => Some(unit::DFG_LUT),
        (0, 5) => Some(unit::INSTANCES),
        (1, binding @ (1 | 3 | 5 | 7 | 9)) => Some(unit::MATERIAL + (binding - 1) / 2),
        _ => None,
    }
}

/// A linked program.
pub struct Program {
    pub raw: glow::Program,
    /// `naga_vs_first_instance`, when the vertex stage reads `instance_index`.
    first_instance: Option<glow::UniformLocation>,
    /// Its value now: uniforms are per-program state.
    first_instance_value: Cell<u32>,
}

impl Program {
    /// Make draws start at instance `first` (the program must be in use).
    pub fn set_first_instance(&self, gpu: &Gpu, first: u32) {
        if let Some(location) = &self.first_instance
            && self.first_instance_value.replace(first) != first
        {
            unsafe { gpu.gl.uniform_1_u32(Some(location), first) };
        }
    }
}

/// A program the browser is compiling and linking.
pub(super) struct Linking {
    label: String,
    program: glow::Program,
    shaders: [glow::Shader; 2],
    /// Blocks and samplers to bind once it has linked.
    blocks: Vec<(String, Binding)>,
    samplers: Vec<(String, Binding)>,
}

impl Linking {
    /// Translate `wgsl` and start compiling its `vertex` and `fragment` entry points.
    pub(super) fn start(
        gpu: &Gpu,
        label: &str,
        wgsl: &str,
        vertex: &str,
        fragment: Option<&str>,
    ) -> Self {
        let entries: Vec<&str> = [Some(vertex), fragment].into_iter().flatten().collect();
        let (sources, blocks, samplers) = match glsl::translate(label, wgsl, &entries) {
            Ok(stages) => {
                let mut blocks = Vec::new();
                let mut samplers = Vec::new();
                let mut sources = Vec::new();
                for stage in stages {
                    blocks.extend(stage.blocks);
                    samplers.extend(stage.samplers);
                    sources.push(stage.source);
                }
                if sources.len() < 2 {
                    sources.push(EMPTY_FRAGMENT.to_owned());
                }
                (sources, blocks, samplers)
            }
            Err(error) => {
                gpu.error
                    .set(format!("WebGL shader translation failed: {error}"));
                let sources = vec![FAILED_VERTEX.to_owned(), EMPTY_FRAGMENT.to_owned()];
                (sources, Vec::new(), Vec::new())
            }
        };
        let gl = &gpu.gl;
        unsafe {
            let program = gpu.created(gl.create_program(), "program");
            let shaders = [glow::VERTEX_SHADER, glow::FRAGMENT_SHADER]
                .map(|stage| gpu.created(gl.create_shader(stage), "shader"));
            for (shader, source) in shaders.iter().zip(&sources) {
                gl.shader_source(*shader, source);
                gl.compile_shader(*shader);
                gl.attach_shader(program, *shader);
            }
            gl.link_program(program);
            Self {
                label: label.to_owned(),
                program,
                shaders,
                blocks,
                samplers,
            }
        }
    }

    /// Whether linking finished; asking does not wait for it.
    pub(super) fn done(&self, gpu: &Gpu) -> bool {
        !gpu.parallel_compile || unsafe { gpu.gl.get_program_completion_status(self.program) }
    }

    /// Check the link (waiting for it if it is still running) and bind the program's
    /// uniform blocks and samplers to their fixed points and units.
    pub(super) fn finish(self, gpu: &Gpu) -> Program {
        let gl = &gpu.gl;
        let program = self.program;
        unsafe {
            if !gl.get_program_link_status(program) {
                let mut log = gl.get_program_info_log(program);
                for shader in self.shaders {
                    if !gl.get_shader_compile_status(shader) {
                        log += &gl.get_shader_info_log(shader);
                    }
                }
                gpu.error.set(format!(
                    "WebGL program {} failed to link: {}",
                    self.label,
                    log.trim()
                ));
            }
            for shader in self.shaders {
                gl.detach_shader(program, shader);
                gl.delete_shader(shader);
            }
            for (name, binding) in &self.blocks {
                if let (Some(index), Some(point)) = (
                    gl.get_uniform_block_index(program, name),
                    block_point(*binding),
                ) {
                    gl.uniform_block_binding(program, index, point);
                }
            }
            gpu.use_program(program);
            for (name, binding) in &self.samplers {
                if let (Some(location), Some(unit)) = (
                    gl.get_uniform_location(program, name),
                    texture_unit(*binding),
                ) {
                    gl.uniform_1_i32(Some(&location), unit as i32);
                }
            }
            Program {
                raw: program,
                first_instance: gl.get_uniform_location(program, FIRST_INSTANCE),
                first_instance_value: Cell::new(0),
            }
        }
    }
}

/// A program and the fixed-function state it draws with.
#[derive(Clone, Copy)]
pub struct Pipeline {
    /// Index into [`Pipelines::program`].
    pub program: u32,
    pub raster: Raster,
}

fn cull(side: Side) -> Cull {
    match side {
        Side::Front => Cull::Back,
        Side::Back => Cull::Front,
        Side::Double => Cull::None,
    }
}

/// The fixed-function state of a surface or shadow pipeline, as WebGPU's
/// (`webgpu/pipelines.rs` `surface_spec`).
fn surface_raster(key: &PipelineKey) -> Raster {
    Raster {
        cull: cull(key.side),
        depth_func: Some(if key.depth_test {
            glow::LEQUAL
        } else {
            glow::ALWAYS
        }),
        depth_write: key.depth_write,
        blend: match key.blend {
            BlendMode::Replace => Blend::Replace,
            BlendMode::Normal => Blend::Normal,
            BlendMode::Additive => Blend::Additive,
        },
        polygon_offset: (key.depth_bias.slope_scale(), key.depth_bias.constant as f32),
        alpha_to_coverage: key.alpha_to_coverage,
    }
}

/// Depth-tested, depth-writing state for `side` (water, merged casters).
fn depth_raster(side: Side) -> Raster {
    Raster {
        cull: cull(side),
        depth_func: Some(glow::LEQUAL),
        depth_write: true,
        ..Raster::PLAIN
    }
}

/// The water, output and merged shadow pipelines every arena draws with.
pub struct FixedPipelines {
    pub water: Pipeline,
    pub output: Pipeline,
    /// Merged casters, depth-only then alpha-tested, indexed by
    /// `shadow_merged_index`.
    pub shadow_merged: Vec<Pipeline>,
}

/// The water, output, merged and cutout caster programs, in that order.
const FIXED_PROGRAMS: usize = 4;

pub struct Pipelines {
    /// The WGSL of each surface or shadow variant.
    sources: HashMap<ShaderKey, String>,
    programs: Vec<Program>,
    by_shader: HashMap<ShaderKey, u32>,
    /// Variants linking in the background.
    linking: HashMap<ShaderKey, Linking>,
    pipelines: Vec<Pipeline>,
    index: HashMap<PipelineKey, u32>,
    /// The fixed programs while they link.
    fixed_jobs: Option<Vec<Linking>>,
    fixed: Option<FixedPipelines>,
}

impl Pipelines {
    /// Start linking the fixed water, output and merged shadow programs; they are
    /// used once linked ([`Self::fixed_ready`]) while the page builds the arena.
    pub fn new(gpu: &Gpu) -> Self {
        let jobs = vec![
            Linking::start(gpu, "water", &water_source(), "vs_water", Some("fs_water")),
            Linking::start(
                gpu,
                "output",
                crate::shader::OUTPUT_WGSL,
                "vs_fullscreen",
                Some("fs_output"),
            ),
            Linking::start(
                gpu,
                "shadow merged",
                &shadow_merged_source(),
                "vs_shadow_merged",
                None,
            ),
            Linking::start(
                gpu,
                "shadow cutout",
                &shadow_cutout_source(),
                "vs_shadow_cutout",
                Some("fs_shadow_cutout"),
            ),
        ];
        Self {
            sources: HashMap::new(),
            programs: Vec::new(),
            by_shader: HashMap::new(),
            linking: HashMap::new(),
            pipelines: Vec::new(),
            index: HashMap::new(),
            fixed_jobs: Some(jobs),
            fixed: None,
        }
    }

    /// Whether the fixed pipelines exist, creating them once their programs linked.
    pub fn fixed_ready(&mut self, gpu: &Gpu) -> bool {
        if self
            .fixed_jobs
            .as_ref()
            .is_some_and(|jobs| jobs.iter().all(|job| job.done(gpu)))
        {
            self.create_fixed(gpu);
        }
        self.fixed.is_some()
    }

    /// Create the fixed pipelines now, waiting for their programs if they have not
    /// linked; returns whether that stalled.
    pub fn ensure_fixed(&mut self, gpu: &Gpu) -> bool {
        let stalled = !self.fixed_ready(gpu);
        if stalled {
            self.create_fixed(gpu);
        }
        stalled
    }

    fn create_fixed(&mut self, gpu: &Gpu) {
        let Some(jobs) = self.fixed_jobs.take() else {
            return;
        };
        debug_assert_eq!(jobs.len(), FIXED_PROGRAMS);
        let first = self.programs.len() as u32;
        for job in jobs {
            let program = job.finish(gpu);
            self.programs.push(program);
        }
        let [water, output, merged, cutout] = [0, 1, 2, 3].map(|offset| first + offset);
        let mut shadow_merged = Vec::new();
        for program in [merged, cutout] {
            for &side in &SHADOW_MERGED_SIDES {
                shadow_merged.push(Pipeline {
                    program,
                    raster: depth_raster(side),
                });
            }
        }
        self.fixed = Some(FixedPipelines {
            water: Pipeline {
                program: water,
                raster: depth_raster(Side::Front),
            },
            output: Pipeline {
                program: output,
                raster: Raster::PLAIN,
            },
            shadow_merged,
        });
    }

    /// The fixed pipelines; draws first make sure of them ([`Self::ensure_fixed`]).
    pub fn fixed(&self) -> &FixedPipelines {
        self.fixed
            .as_ref()
            .expect("fixed pipelines are created before drawing")
    }

    pub fn find(&self, key: &PipelineKey) -> Option<u32> {
        self.index.get(key).copied()
    }

    pub fn get(&self, index: u32) -> &Pipeline {
        &self.pipelines[index as usize]
    }

    pub fn program(&self, index: u32) -> &Program {
        &self.programs[index as usize]
    }

    /// Start linking the program of `shader` unless it is linked or linking.
    fn start(&mut self, gpu: &Gpu, effects: &EffectRegistry, shader: ShaderKey) {
        if self.by_shader.contains_key(&shader) || self.linking.contains_key(&shader) {
            return;
        }
        let (vertex, fragment) = match shader.pass {
            Pass::Main => ("vs_main", Some("fs_main")),
            Pass::Shadow => (
                "vs_shadow",
                shader.shadow_needs_fragment(effects).then_some("fs_shadow"),
            ),
        };
        let source = self
            .sources
            .entry(shader)
            .or_insert_with(|| shader_source(&shader, effects));
        let label = format!("{:?} {}", shader.pass, shader.effect);
        let job = Linking::start(gpu, &label, source, vertex, fragment);
        self.linking.insert(shader, job);
    }

    /// The linked program of `shader`, waiting for its link if `wait`.
    fn linked(&mut self, gpu: &Gpu, shader: ShaderKey, wait: bool) -> Option<u32> {
        if let Some(&program) = self.by_shader.get(&shader) {
            return Some(program);
        }
        if !wait && !self.linking.get(&shader)?.done(gpu) {
            return None;
        }
        let program = self.linking.remove(&shader)?.finish(gpu);
        self.programs.push(program);
        let index = self.programs.len() as u32 - 1;
        self.by_shader.insert(shader, index);
        Some(index)
    }

    fn create(&mut self, key: &PipelineKey, program: u32) -> u32 {
        self.pipelines.push(Pipeline {
            program,
            raster: surface_raster(key),
        });
        let index = self.pipelines.len() as u32 - 1;
        self.index.insert(*key, index);
        index
    }

    /// The pipeline for a key once its program has linked, created then only if
    /// `create` (the caller's per-step budget); until then it starts the link and
    /// returns `None`. Without background linking the link runs now, within budget.
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
        self.start(gpu, effects, key.shader);
        if !create {
            return None;
        }
        let program = self.linked(gpu, key.shader, !gpu.parallel_compile)?;
        Some(self.create(key, program))
    }

    /// The pipeline for a key, linking it synchronously on first use (a stall on a
    /// cold shader cache; preparation uses [`Self::request`]).
    pub fn ensure(&mut self, gpu: &Gpu, effects: &EffectRegistry, key: &PipelineKey) -> u32 {
        if let Some(index) = self.find(key) {
            return index;
        }
        self.start(gpu, effects, key.shader);
        let program = self
            .linked(gpu, key.shader, true)
            .expect("a started program links");
        self.create(key, program)
    }

    /// Variants linking in the background.
    pub fn compiling(&self) -> u32 {
        self.linking.len() as u32
    }

    /// Pipelines including the fixed water, output and merged shadow ones.
    pub fn count(&self) -> usize {
        self.pipelines.len()
            + self
                .fixed
                .as_ref()
                .map_or(0, |fixed| 2 + fixed.shadow_merged.len())
    }

    /// Distinct shader sources, including the fixed water, output and merged shadow
    /// ones.
    pub fn module_count(&self) -> usize {
        self.sources.len() + FIXED_PROGRAMS
    }
}
