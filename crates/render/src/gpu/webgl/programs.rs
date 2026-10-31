//! GL programs of the variants, from the GLSL the build translated from their WGSL
//! (`shader/tables.rs`), and pipelines: a program plus the
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

use super::context::{Gpu, Raster};
use crate::effects::EffectRegistry;
use crate::gpu::{FixedPipelines, OCCLUSION_DOWNSCALE, SHADOW_MERGED_SIDES};
use crate::shader::glsl::{self, Binding, block_point, texture_unit};
use crate::shader::tables::page::{fixed_program, variant_program};
use crate::shader::{
    BlendMode, FixedShader, InstanceSource, PipelineKey, ShaderKey, occlusion_scale,
};

/// The fragment stage of programs whose WGSL has none (depth-only casters).
const EMPTY_FRAGMENT: &str = "#version 300 es\nvoid main(void) {}\n";

// The occlusion shader was built for the copy of the depth it reads.
const _: () = assert!(OCCLUSION_DOWNSCALE == occlusion_scale(InstanceSource::Texture));

/// The uniform naga adds to `gl_InstanceID` for WGSL's `instance_index`
/// (`naga::back::glsl::FIRST_INSTANCE_BINDING`): WebGL2 has no base instance.
const FIRST_INSTANCE: &str = "naga_vs_first_instance";

/// The WGSL bind group of a material's (or the water's) uniform and textures.
const MATERIAL_GROUP: u32 = 1;

/// A linked program.
pub struct Program {
    pub raw: glow::Program,
    /// Whether it reads the material's uniform or textures: most shadow casters
    /// read neither, so their draws bind no material.
    pub reads_material: bool,
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
    /// Start compiling `program`, the GLSL the build translated (`shader/tables.rs`).
    pub(super) fn start(gpu: &Gpu, label: &str, program: glsl::Program) -> Self {
        let glsl::Program {
            vertex,
            fragment,
            blocks,
            samplers,
        } = program;
        let sources = [
            vertex,
            fragment.unwrap_or_else(|| EMPTY_FRAGMENT.to_owned()),
        ];
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
            // naga declares only the bindings an entry point uses.
            let reads_material = self
                .blocks
                .iter()
                .chain(&self.samplers)
                .any(|&(_, (group, _))| group == MATERIAL_GROUP);
            Program {
                raw: program,
                reads_material,
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

/// The fixed-function state of a surface or shadow pipeline, as WebGPU's
/// (`webgpu/pipelines.rs` `surface_spec`).
fn surface_raster(key: &PipelineKey) -> Raster {
    Raster {
        side: key.side,
        depth_func: Some(if key.depth_test {
            glow::LEQUAL
        } else {
            glow::ALWAYS
        }),
        depth_write: key.depth_write,
        blend: key.blend,
        polygon_offset: (key.depth_bias.slope_scale(), key.depth_bias.constant as f32),
        alpha_to_coverage: key.shader.alpha_to_coverage,
    }
}

/// Depth-tested, depth-writing state for `side` (water, merged casters).
fn depth_raster(side: Side) -> Raster {
    Raster {
        side,
        depth_func: Some(glow::LEQUAL),
        depth_write: true,
        ..Raster::PLAIN
    }
}

/// The output, sky, bloom (all three passes and the occlusion's blur), ambient
/// occlusion, merged and cutout caster programs, in that order.
const FIXED_PROGRAMS: usize = 6;

pub struct Pipelines {
    programs: Vec<Program>,
    by_shader: HashMap<ShaderKey, u32>,
    /// Variants linking in the background.
    linking: HashMap<ShaderKey, Linking>,
    pipelines: Vec<Pipeline>,
    index: HashMap<PipelineKey, u32>,
    /// The fixed programs while they link.
    fixed_jobs: Option<[Linking; FIXED_PROGRAMS]>,
    fixed: Option<FixedPipelines<Pipeline>>,
    /// The water's program while it links, from the first arena with water on.
    water_job: Option<Linking>,
    water: Option<Pipeline>,
}

impl Pipelines {
    /// Start linking the fixed output, sky, bloom, occlusion and merged shadow
    /// programs; they are used once linked ([`Self::fixed_ready`]) while the page
    /// builds the arena.
    pub fn new(gpu: &Gpu) -> Self {
        let fixed = |label, shader, vertex, fragment| {
            Linking::start(gpu, label, fixed_program(shader, vertex, fragment))
        };
        let jobs = [
            fixed(
                "output",
                FixedShader::Output,
                "vs_fullscreen",
                Some("fs_output"),
            ),
            fixed("sky", FixedShader::Sky, "vs_sky", Some("fs_sky")),
            // Each pass draws as its own instance index (`BloomPass`).
            fixed("bloom", FixedShader::Output, "vs_post", Some("fs_bloom")),
            fixed(
                "ambient occlusion",
                FixedShader::Occlusion,
                "vs_occlusion",
                Some("fs_occlusion"),
            ),
            fixed(
                "shadow merged",
                FixedShader::ShadowMerged,
                "vs_shadow_merged",
                None,
            ),
            fixed(
                "shadow cutout",
                FixedShader::ShadowCutout,
                "vs_shadow_cutout",
                Some("fs_shadow_cutout"),
            ),
        ];
        Self {
            programs: Vec::new(),
            by_shader: HashMap::new(),
            linking: HashMap::new(),
            pipelines: Vec::new(),
            index: HashMap::new(),
            fixed_jobs: Some(jobs),
            fixed: None,
            water_job: None,
            water: None,
        }
    }

    /// Start linking the water's program unless it is linked or linking: called once
    /// an arena has water.
    pub fn start_water(&mut self, gpu: &Gpu) {
        if self.water.is_none() && self.water_job.is_none() {
            let program = fixed_program(FixedShader::Water, "vs_water", Some("fs_water"));
            self.water_job = Some(Linking::start(gpu, "water", program));
        }
    }

    /// Whether the water's pipeline exists, creating it once its program linked (and
    /// starting the link if needed).
    pub fn water_ready(&mut self, gpu: &Gpu) -> bool {
        self.start_water(gpu);
        if let Some(job) = self.water_job.take_if(|job| job.done(gpu)) {
            self.create_water(gpu, job);
        }
        self.water.is_some()
    }

    /// Create the water's pipeline now, waiting for its program if it has not linked;
    /// returns whether that stalled.
    pub fn ensure_water(&mut self, gpu: &Gpu) -> bool {
        let stalled = !self.water_ready(gpu);
        if let Some(job) = self.water_job.take() {
            self.create_water(gpu, job);
        }
        stalled
    }

    fn create_water(&mut self, gpu: &Gpu, job: Linking) {
        self.programs.push(job.finish(gpu));
        self.water = Some(Pipeline {
            program: self.programs.len() as u32 - 1,
            raster: depth_raster(Side::Front),
        });
    }

    /// The water surface's pipeline; an arena with water makes sure of it before
    /// drawing ([`Self::ensure_water`]).
    pub fn water(&self) -> &Pipeline {
        self.water
            .as_ref()
            .expect("the water pipeline before water draws")
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
        let [output, sky, bloom, occlusion, merged, cutout] = jobs.map(|job| {
            self.programs.push(job.finish(gpu));
            self.programs.len() as u32 - 1
        });
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
            output: Pipeline {
                program: output,
                raster: Raster::PLAIN,
            },
            // Depth-tested on the far plane without writing: only clear pixels.
            sky: Pipeline {
                program: sky,
                raster: Raster {
                    depth_func: Some(glow::LEQUAL),
                    ..Raster::PLAIN
                },
            },
            bloom: [
                Pipeline {
                    program: bloom,
                    raster: Raster::PLAIN,
                },
                Pipeline {
                    program: bloom,
                    raster: Raster::PLAIN,
                },
                Pipeline {
                    program: bloom,
                    raster: Raster {
                        blend: BlendMode::Additive,
                        ..Raster::PLAIN
                    },
                },
            ],
            occlusion: Pipeline {
                program: occlusion,
                raster: Raster::PLAIN,
            },
            // Drawn as `BloomPass::OcclusionBlur`.
            occlusion_blur: Pipeline {
                program: bloom,
                raster: Raster::PLAIN,
            },
            // Drawn as `BloomPass::BackdropRows` and `BackdropColumns`.
            backdrop: Pipeline {
                program: bloom,
                raster: Raster::PLAIN,
            },
            shadow_merged,
        });
    }

    /// The fixed pipelines; draws first make sure of them ([`Self::ensure_fixed`]).
    pub fn fixed(&self) -> &FixedPipelines<Pipeline> {
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
        let label = format!("{:?} {}", shader.pass, shader.effect);
        let job = Linking::start(gpu, &label, variant_program(&shader, effects));
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

    /// Pipelines including the fixed output and merged shadow ones and the water's.
    pub fn count(&self) -> usize {
        self.pipelines.len()
            + self.fixed.as_ref().map_or(0, FixedPipelines::count)
            + usize::from(self.water.is_some())
    }

    /// Distinct shader sources, including the fixed output, sky, bloom and merged
    /// shadow ones and the water's.
    pub fn module_count(&self) -> usize {
        self.by_shader.len()
            + self.linking.len()
            + FIXED_PROGRAMS
            + usize::from(self.water.is_some())
    }
}
