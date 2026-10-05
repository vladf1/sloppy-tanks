//! Programs are cached by shader variant; pipeline entries add only the game's
//! raster state. No command recording or emulation of a general GPU API.
use crate::gpu::gl::device::{Device, Raster};
use crate::{
    effects::EffectRegistry,
    shader::{
        self, Pass, PipelineKey, ShaderKey, shader_source, shadow_cutout_source,
        shadow_merged_source, water_source,
    },
};
use glow::HasContext;
use sloppy_core::scene::Side;
use std::{cell::Cell, collections::HashMap, rc::Rc};

pub const SAMPLE_COUNT: u32 = 4;
pub fn shadow_merged_index(side: Side, cutout: bool) -> usize {
    (match side {
        Side::Front => 0,
        Side::Back => 1,
        Side::Double => 2,
    }) + if cutout { 3 } else { 0 }
}
pub struct Program {
    device: Device,
    pub raw: glow::Program,
    first: Option<glow::UniformLocation>,
    last_first: Cell<Option<u32>>,
}
impl Drop for Program {
    fn drop(&mut self) {
        self.device.delete_program(self.raw);
    }
}
impl Program {
    fn new(device: &Device, code: &str, vertex: &str, fragment: Option<&str>) -> Rc<Self> {
        let gl = &device.gl;
        let raw = unsafe { gl.create_program().expect("GL program") };
        let result = (|| -> Result<(), String> {
            let mut bindings = Vec::with_capacity(2);
            for (stage, entry, code, ty) in [
                (naga::ShaderStage::Vertex, vertex, code, glow::VERTEX_SHADER),
                (
                    naga::ShaderStage::Fragment,
                    fragment.unwrap_or("empty"),
                    if fragment.is_some() {
                        code
                    } else {
                        crate::glsl::EMPTY_FRAGMENT
                    },
                    glow::FRAGMENT_SHADER,
                ),
            ] {
                let translated = crate::glsl::translate(code, entry, stage)?;
                unsafe {
                    let shader = gl.create_shader(ty)?;
                    gl.shader_source(shader, &translated.source);
                    gl.compile_shader(shader);
                    if !gl.get_shader_compile_status(shader) {
                        let log = gl.get_shader_info_log(shader);
                        gl.delete_shader(shader);
                        return Err(format!("{entry}: {log}"));
                    }
                    gl.attach_shader(raw, shader);
                    gl.delete_shader(shader);
                }
                bindings.push((translated.blocks, translated.textures));
            }
            unsafe {
                gl.link_program(raw);
                if !gl.get_program_link_status(raw) {
                    return Err(gl.get_program_info_log(raw));
                }
            }
            device.program(raw);
            for (blocks, textures) in bindings {
                unsafe {
                    for b in blocks {
                        if let Some(index) = gl.get_uniform_block_index(raw, &b.name) {
                            gl.uniform_block_binding(raw, index, b.slot);
                        }
                    }
                    for t in textures {
                        gl.uniform_1_i32(
                            gl.get_uniform_location(raw, &t.name).as_ref(),
                            t.slot as i32,
                        );
                    }
                }
            }
            Ok(())
        })();
        let first = match result {
            Ok(()) => unsafe { gl.get_uniform_location(raw, "naga_vs_first_instance") },
            Err(error) => {
                device.fail(format!("WebGL shader {vertex}: {error}"));
                None
            }
        };
        Rc::new(Self {
            device: device.clone(),
            raw,
            first,
            last_first: Cell::new(None),
        })
    }
    fn first(&self, value: u32) {
        if self.last_first.get() != Some(value) {
            if let Some(location) = &self.first {
                unsafe {
                    self.device.gl.uniform_1_u32(Some(location), value);
                }
            }
            self.last_first.set(Some(value));
        }
    }
}
pub struct Pipeline {
    pub program: Rc<Program>,
    pub raster: Raster,
    pub extras: u8,
    pub merged: bool,
}
impl Pipeline {
    pub fn bind(&self, device: &Device, first: u32) {
        device.program(self.program.raw);
        device.raster(self.raster);
        self.program.first(first);
    }
}
pub struct FixedPipelines {
    pub water: Pipeline,
    pub output: Pipeline,
    pub copy_depth: Pipeline,
    pub shadow_merged: Vec<Pipeline>,
}
pub struct Pipelines {
    programs: HashMap<ShaderKey, Rc<Program>>,
    pipelines: Vec<Pipeline>,
    index: HashMap<PipelineKey, u32>,
    ranks: HashMap<PipelineKey, u32>,
    fixed: FixedPipelines,
}
impl Pipelines {
    pub fn new(device: &Device) -> Self {
        let output = Pipeline {
            program: Program::new(
                device,
                shader::OUTPUT_WGSL,
                "vs_fullscreen",
                Some("fs_output"),
            ),
            raster: Raster {
                side: Side::Double,
                depth_test: false,
                depth_write: false,
                ..Default::default()
            },
            extras: 0,
            merged: false,
        };
        let water = Pipeline {
            program: Program::new(device, &water_source(), "vs_water", Some("fs_water")),
            raster: Raster::default(),
            extras: 0,
            merged: false,
        };
        let copy_depth = Pipeline {
            program: Program::new(
                device,
                shader::DEPTH_COPY_WGSL,
                "vs_fullscreen",
                Some("fs_copy_depth"),
            ),
            raster: Raster {
                side: Side::Double,
                depth_test: false,
                ..Default::default()
            },
            extras: 0,
            merged: false,
        };
        let mut shadow_merged = Vec::new();
        for cutout in [false, true] {
            let program = if cutout {
                Program::new(
                    device,
                    &shadow_cutout_source(),
                    "vs_shadow_cutout",
                    Some("fs_shadow_cutout"),
                )
            } else {
                Program::new(device, &shadow_merged_source(), "vs_shadow_merged", None)
            };
            for side in [Side::Front, Side::Back, Side::Double] {
                shadow_merged.push(Pipeline {
                    program: program.clone(),
                    raster: Raster {
                        side,
                        ..Default::default()
                    },
                    extras: 0,
                    merged: true,
                });
            }
        }
        Self {
            programs: HashMap::new(),
            pipelines: Vec::new(),
            index: HashMap::new(),
            ranks: HashMap::new(),
            fixed: FixedPipelines {
                water,
                output,
                copy_depth,
                shadow_merged,
            },
        }
    }
    pub fn fixed_ready(&mut self, _: &Device) -> bool {
        true
    }
    pub fn ensure_fixed(&mut self, _: &Device) -> bool {
        false
    }
    pub fn fixed(&self) -> &FixedPipelines {
        &self.fixed
    }
    pub fn find(&self, key: &PipelineKey) -> Option<u32> {
        self.index.get(key).copied()
    }
    pub fn rank(&mut self, key: &PipelineKey) -> u32 {
        let next = self.ranks.len() as u32;
        *self.ranks.entry(*key).or_insert(next)
    }
    pub fn get(&self, index: u32) -> &Pipeline {
        &self.pipelines[index as usize]
    }
    pub fn request(
        &mut self,
        device: &Device,
        effects: &EffectRegistry,
        key: &PipelineKey,
        create: bool,
    ) -> Option<u32> {
        self.find(key)
            .or_else(|| create.then(|| self.ensure(device, effects, key)))
    }
    pub fn ensure(&mut self, device: &Device, effects: &EffectRegistry, key: &PipelineKey) -> u32 {
        if let Some(index) = self.find(key) {
            return index;
        }
        let program = self
            .programs
            .entry(key.shader)
            .or_insert_with(|| {
                let main = key.shader.pass == Pass::Main;
                Program::new(
                    device,
                    &shader_source(&key.shader, effects),
                    if main { "vs_main" } else { "vs_shadow" },
                    if main {
                        Some("fs_main")
                    } else if key.shader.shadow_needs_fragment(effects) {
                        Some("fs_shadow")
                    } else {
                        None
                    },
                )
            })
            .clone();
        let index = self.pipelines.len() as u32;
        self.pipelines.push(Pipeline {
            program,
            raster: Raster {
                side: key.side,
                blend: key.blend,
                depth_test: key.depth_test,
                depth_write: key.depth_write,
                bias: key.depth_bias,
                coverage: key.alpha_to_coverage,
            },
            extras: key.shader.extra_attributes,
            merged: false,
        });
        self.index.insert(*key, index);
        index
    }
    pub fn compiling(&self) -> u32 {
        0
    }
    pub fn count(&self) -> usize {
        self.pipelines.len() + 9
    }
    pub fn module_count(&self) -> usize {
        self.programs.len() + 5
    }
}
