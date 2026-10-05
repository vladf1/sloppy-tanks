//! Direct WebGL2 submission for the game's shared scene and draw lists.
//! `glow` owns GL object names. Only buffers and the game's fixed resource groups
//! cross this boundary; there is no command encoder or generic binding API.
pub(super) mod device;
mod targets;
use super::*;
pub(super) use device::{
    Buffer, Device, Queue, base_buffer, mesh_buffer, uniform_buffer, write_buffer,
    write_material_uniform,
};
use device::{Sampler, Texture};
use glow::HasContext;
use std::cell::Cell;
pub(super) use targets::ColorTarget;
use targets::{DepthTarget, OutputTarget};

pub struct Context {
    pub device: Device,
    pub queue: Queue,
    canvas: web_sys::HtmlCanvasElement,
}
pub struct FrameGroup {
    uniform: Buffer,
    shadow: Texture,
    lut: Texture,
    instances: Texture,
    shadow_sampler: Sampler,
    lut_sampler: Sampler,
}
pub struct MaterialGroup {
    pub uniform: Buffer,
    pub maps: [Texture; 5],
    pub samplers: [Sampler; 5],
}
pub struct WaterGroup {
    uniform: Buffer,
    normals: Texture,
    reflection: Texture,
    normal_sampler: Sampler,
    reflection_sampler: Sampler,
}
impl FrameGroup {
    fn bind(&self, gl: &Device) {
        gl.block(0, &self.uniform);
        gl.texture(0, &self.shadow, Some(&self.shadow_sampler));
        gl.texture(1, &self.lut, Some(&self.lut_sampler));
        gl.texture(2, &self.instances, None);
    }
}
impl MaterialGroup {
    fn bind(&self, gl: &Device) {
        gl.block(1, &self.uniform);
        for (i, (t, s)) in self.maps.iter().zip(&self.samplers).enumerate() {
            gl.texture(3 + i as u32, t, Some(s));
        }
    }
}
impl WaterGroup {
    fn bind(&self, gl: &Device) {
        gl.block(1, &self.uniform);
        gl.texture(3, &self.normals, Some(&self.normal_sampler));
        gl.texture(4, &self.reflection, Some(&self.reflection_sampler));
    }
}
pub(super) struct FrameResources {
    pub ctx: Context,
    pub main_target: ColorTarget,
    shadow_map: DepthTarget,
    static_shadow_map: DepthTarget,
    dummy_depth: DepthTarget,
    shadow_sampler: Sampler,
    lut_view: Texture,
    lut_sampler: Sampler,
    reflection_sampler: Sampler,
    nearest_sampler: Sampler,
    pub view_uniforms: [Buffer; VIEW_COUNT],
    pub view_groups: Vec<FrameGroup>,
    pub instance_records: InstanceStore,
    pub output_uniform: Buffer,
    output: OutputTarget,
    pub shadow_base_buffer: Buffer,
    pub shadow_base_capacity: u32,
    fence: Cell<Option<glow::Fence>>,
}
impl FrameResources {
    pub async fn new(canvas: web_sys::HtmlCanvasElement) -> Result<Self, String> {
        let device = Device::new(&canvas)?;
        let (width, height) = (canvas.width().max(1), canvas.height().max(1));
        let lut = Texture::new(
            &device,
            glow::RG16F,
            lut::DFG_LUT_SIZE,
            lut::DFG_LUT_SIZE,
            1,
        );
        unsafe {
            device.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                lut::DFG_LUT_SIZE as i32,
                lut::DFG_LUT_SIZE as i32,
                glow::RG,
                glow::HALF_FLOAT,
                glow::PixelUnpackData::Slice(Some(bytemuck::cast_slice(&lut::DFG_LUT))),
            );
        }
        let shadow_size = SunShadow::default().map_size;
        let result = Self {
            main_target: ColorTarget::new(&device, "main", width, height, SAMPLE_COUNT),
            shadow_map: DepthTarget::new(&device, shadow_size),
            static_shadow_map: DepthTarget::new(&device, shadow_size),
            dummy_depth: DepthTarget::new(&device, 1),
            shadow_sampler: Sampler::new(&device, true, false, glow::CLAMP_TO_EDGE, true, 1.),
            lut_view: lut,
            lut_sampler: Sampler::new(&device, true, false, glow::CLAMP_TO_EDGE, false, 1.),
            reflection_sampler: Sampler::new(&device, true, false, glow::CLAMP_TO_EDGE, false, 1.),
            nearest_sampler: Sampler::new(&device, false, false, glow::CLAMP_TO_EDGE, false, 1.),
            view_uniforms: [0, 1, 2]
                .map(|_| uniform_buffer(&device, "frame", size_of::<FrameUniform>() as u64)),
            view_groups: Vec::new(),
            instance_records: InstanceStore::new(&device, "instances", INITIAL_INSTANCE_CAPACITY),
            output_uniform: uniform_buffer(&device, "output", 16),
            output: OutputTarget::new(&device, width, height),
            shadow_base_buffer: base_buffer(&device, INITIAL_SHADOW_BASES),
            shadow_base_capacity: INITIAL_SHADOW_BASES,
            fence: Cell::new(None),
            ctx: Context {
                queue: device.clone(),
                device,
                canvas,
            },
        };
        result.ctx.device.check("initialization");
        if let Some(error) = result.error() {
            return Err(error);
        }
        Ok(result)
    }
    pub fn size(&self) -> (u32, u32) {
        (self.main_target.width, self.main_target.height)
    }
    pub fn error(&self) -> Option<String> {
        self.ctx.device.error()
    }
}
impl Drop for FrameResources {
    fn drop(&mut self) {
        if let Some(fence) = self.fence.take() {
            unsafe {
                self.ctx.device.gl.delete_sync(fence);
            }
        }
    }
}
impl Renderer {
    pub(super) fn frame_group(&self, view: usize, instances: &InstanceStore) -> FrameGroup {
        FrameGroup {
            uniform: self.gpu.view_uniforms[view].clone(),
            shadow: if view == SHADOW_VIEW {
                &self.gpu.dummy_depth.texture
            } else {
                &self.gpu.shadow_map.texture
            }
            .clone(),
            lut: self.gpu.lut_view.clone(),
            instances: instances.texture.clone(),
            shadow_sampler: self.gpu.shadow_sampler.clone(),
            lut_sampler: self.gpu.lut_sampler.clone(),
        }
    }
    pub(super) fn water_group(
        &mut self,
        normals: &TextureRef,
        uniform: &Buffer,
        target: &ColorTarget,
    ) -> (WaterGroup, bool) {
        let normal_sampler = self.textures.sampler(&self.gpu.ctx.device, Some(normals));
        let (view, ready) = self.textures.view(normals);
        (
            WaterGroup {
                uniform: uniform.clone(),
                normals: view.clone(),
                normal_sampler,
                reflection: target.resolved.clone(),
                reflection_sampler: self.gpu.reflection_sampler.clone(),
            },
            !ready,
        )
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        let gl = &self.gpu.ctx.device;
        let (width, height) = (
            width.clamp(1, gl.max_texture),
            height.clamp(1, gl.max_texture),
        );
        if self.gpu.size() == (width, height) {
            return;
        }
        self.gpu.ctx.canvas.set_width(width);
        self.gpu.ctx.canvas.set_height(height);
        self.gpu.main_target.destroy();
        self.gpu.main_target = ColorTarget::new(gl, "main", width, height, SAMPLE_COUNT);
        self.gpu.output = OutputTarget::new(gl, width, height);
        gl.check("resize");
    }
    pub(super) fn resize_shadow(&mut self, size: u32) {
        let gl = &self.gpu.ctx.device;
        self.gpu.shadow_map.destroy();
        self.gpu.static_shadow_map.destroy();
        self.gpu.shadow_map = DepthTarget::new(gl, size.max(1));
        self.gpu.static_shadow_map = DepthTarget::new(gl, size.max(1));
        self.rebuild_view_groups();
    }
    pub fn await_gpu(&mut self) {
        let gl = &self.gpu.ctx.device;
        unsafe {
            if let Some(f) = self.gpu.fence.take() {
                gl.gl.delete_sync(f);
            }
            match gl.gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0) {
                Ok(f) => self.gpu.fence.set(Some(f)),
                Err(e) => gl.fail(format!("WebGL fence: {e}")),
            }
            gl.gl.flush();
        }
    }
    pub fn gpu_idle(&self) -> bool {
        let Some(f) = self.gpu.fence.get() else {
            return true;
        };
        let gl = &self.gpu.ctx.device;
        unsafe {
            match gl.gl.client_wait_sync(f, 0, 0) {
                glow::ALREADY_SIGNALED | glow::CONDITION_SATISFIED => {
                    gl.gl.delete_sync(f);
                    self.gpu.fence.set(None);
                    true
                }
                glow::WAIT_FAILED => {
                    gl.fail("WebGL GPU wait failed".into());
                    true
                }
                _ => false,
            }
        }
    }
    pub(super) fn warm_output(&mut self) {
        if self.error().is_none() {
            self.encode_scene(false);
            let target = OutputTarget::new(&self.gpu.ctx.device, 4, 4);
            self.encode_output(&target);
            self.gpu.ctx.device.check("warm-up");
        }
    }
    pub(super) fn draw_output(&mut self) -> Result<(), String> {
        if let Some(error) = self.error() {
            return Err(error);
        }
        self.encode_scene(self.reflection_active);
        self.encode_output(&self.gpu.output);
        self.gpu.output.present();
        self.gpu.ctx.device.check_frame();
        self.error().map_or(Ok(()), Err)
    }
    fn encode_output(&self, target: &OutputTarget) {
        let gl = &self.gpu.ctx.device;
        target.begin();
        let output = &self.pipelines.fixed().output;
        output.bind(gl, 0);
        gl.block(0, &self.gpu.output_uniform);
        gl.texture(
            0,
            &self.gpu.main_target.resolved,
            Some(&self.gpu.nearest_sampler),
        );
        gl.no_vertices();
        unsafe {
            gl.gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }
    fn encode_scene(&mut self, reflection: bool) {
        let gl = &self.gpu.ctx.device;
        let stats = &mut self.stats;
        stats.draw_calls = 0;
        stats.triangles = 0;
        let rgb = hex_to_linear(self.environment.background);
        let clear = [rgb[0], rgb[1], rgb[2], 1.];
        let draws = DrawContext {
            classes: &self.classes,
            meshes: &self.meshes,
            materials: &self.materials,
            pipelines: &self.pipelines,
            pools: &self.pools,
            models: &self.models,
            frame_groups: &self.gpu.view_groups,
            gl,
        };
        let mut static_count = 0;
        if self.cache_static_shadow && self.static_shadow_dirty && self.sun_shadow.enabled {
            self.gpu.static_shadow_map.begin();
            static_count += draws.encode(&self.static_shadow_draws, SHADOW_VIEW, stats);
            static_count += draws.encode_merged(
                &self.static_merged_draws,
                &self.gpu.shadow_base_buffer,
                stats,
            );
        }
        self.gpu.shadow_map.begin();
        if self.sun_shadow.enabled && self.cache_static_shadow {
            let copy = &self.pipelines.fixed().copy_depth;
            copy.bind(gl, 0);
            gl.texture(
                0,
                &self.gpu.static_shadow_map.texture,
                Some(&self.gpu.nearest_sampler),
            );
            gl.no_vertices();
            unsafe {
                gl.gl.draw_arrays(glow::TRIANGLES, 0, 3);
            }
            self.static_shadow_dirty = false;
        }
        stats.shadow_draw_calls = draws.encode(&self.views[SHADOW_VIEW].opaque, SHADOW_VIEW, stats)
            + draws.encode_merged(&self.merged_draws, &self.gpu.shadow_base_buffer, stats)
            + static_count;
        stats.shadow_triangles = stats.triangles;
        stats.reflection_draw_calls = 0;
        stats.reflection_triangles = 0;
        if let (true, Some(water)) = (reflection, &self.water) {
            water.target.begin(clear);
            let view = &self.views[REFLECTION_VIEW];
            stats.reflection_draw_calls = draws.encode(&view.opaque, REFLECTION_VIEW, stats)
                + draws.encode(&view.transparent, REFLECTION_VIEW, stats);
            stats.reflection_triangles = stats.triangles - stats.shadow_triangles;
            water.target.resolve();
        }
        self.gpu.main_target.begin(clear);
        let view = &self.views[MAIN_VIEW];
        draws.encode(&view.opaque, MAIN_VIEW, stats);
        if let Some(water) = &self.water {
            let range = self.meshes.get(water.mesh).range;
            if !range.is_empty() {
                self.gpu.view_groups[MAIN_VIEW].bind(gl);
                water.bind_group.bind(gl);
                let pipeline = &self.pipelines.fixed().water;
                pipeline.bind(gl, 0);
                draws.mesh(range, pipeline, None);
                unsafe {
                    gl.gl.draw_elements_instanced(
                        glow::TRIANGLES,
                        range.index_count as i32,
                        glow::UNSIGNED_INT,
                        (range.first_index * 4) as i32,
                        1,
                    );
                }
                stats.draw_calls += 1;
                stats.triangles += range.index_count as u64 / 3;
            }
        }
        draws.encode(&view.transparent, MAIN_VIEW, stats);
        stats.main_triangles =
            stats.triangles - stats.shadow_triangles - stats.reflection_triangles;
        self.gpu.main_target.resolve();
        stats.draw_calls += 1;
    }
}
struct DrawContext<'a> {
    classes: &'a [Option<ClassEntry>],
    meshes: &'a MeshStore,
    materials: &'a MaterialStore,
    pipelines: &'a Pipelines,
    pools: &'a Slab<PoolEntry>,
    models: &'a Slab<ModelEntry>,
    frame_groups: &'a [FrameGroup],
    gl: &'a Device,
}
impl DrawContext<'_> {
    fn mesh(
        &self,
        range: MeshRange,
        pipeline: &super::pipelines::Pipeline,
        bases: Option<(&Buffer, u32)>,
    ) {
        let (vertices, extra) = self.meshes.vertex_buffers(range.vertex_page);
        self.gl
            .vertices(vertices, extra, bases, pipeline.extras, pipeline.merged);
        self.gl.buffer(
            glow::ELEMENT_ARRAY_BUFFER,
            self.meshes.index_buffer(range.index_page).raw(),
        );
    }
    fn encode_merged(&self, draws: &[MergedDraw], bases: &Buffer, stats: &mut RenderStats) -> u32 {
        self.frame_groups[SHADOW_VIEW].bind(self.gl);
        let mut count = 0;
        for draw in draws {
            let Some(mesh) = self
                .models
                .at(draw.model)
                .and_then(|m| m.shadow.get(draw.group as usize))
                .filter(|m| !m.range.is_empty())
            else {
                continue;
            };
            let pipeline = &self.pipelines.fixed().shadow_merged[mesh.pipeline];
            pipeline.bind(self.gl, 0);
            if let Some(material) = mesh.material {
                self.materials.get(material).bind_group.bind(self.gl);
            }
            self.mesh(mesh.range, pipeline, Some((bases, draw.first)));
            unsafe {
                self.gl.gl.draw_elements_instanced(
                    glow::TRIANGLES,
                    mesh.range.index_count as i32,
                    glow::UNSIGNED_INT,
                    (mesh.range.first_index * 4) as i32,
                    draw.count as i32,
                );
            }
            count += 1;
            stats.triangles += (mesh.range.index_count / 3) as u64 * draw.count as u64;
        }
        stats.draw_calls += count;
        count
    }
    fn encode(&self, draws: &[Draw], view: usize, stats: &mut RenderStats) -> u32 {
        let mut count = 0;
        let mut pool = None;
        let mut material = None;
        self.frame_groups[view].bind(self.gl);
        for draw in draws {
            let Some(class) = &self.classes[draw.class as usize] else {
                continue;
            };
            let range = self.meshes.get(class.key.mesh).range;
            if range.is_empty() {
                continue;
            }
            if pool != class.pool {
                match class.pool {
                    Some(p) => {
                        let Some(entry) = self.pools.at(p) else {
                            continue;
                        };
                        entry.groups[view].bind(self.gl);
                    }
                    None => self.frame_groups[view].bind(self.gl),
                }
                pool = class.pool;
            }
            let Some(pipeline) = (if view == SHADOW_VIEW {
                class.shadow
            } else {
                class.main
            }) else {
                continue;
            };
            if material != Some(class.key.material) {
                self.materials
                    .get(class.key.material)
                    .bind_group
                    .bind(self.gl);
                material = Some(class.key.material);
            }
            let back = if view == SHADOW_VIEW {
                None
            } else {
                class.back
            };
            for index in back.into_iter().chain([pipeline]) {
                let pipeline = self.pipelines.get(index);
                pipeline.bind(self.gl, draw.first_instance);
                self.mesh(range, pipeline, None);
                unsafe {
                    self.gl.gl.draw_elements_instanced(
                        glow::TRIANGLES,
                        range.index_count as i32,
                        glow::UNSIGNED_INT,
                        (range.first_index * 4) as i32,
                        draw.instance_count as i32,
                    );
                }
                count += 1;
                stats.triangles += (range.index_count / 3) as u64 * draw.instance_count as u64;
            }
        }
        stats.draw_calls += count;
        count
    }
}
