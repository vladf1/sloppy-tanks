//! The WebGL2 backend: glow on the browser's WebGL2 context, without wgpu. It draws
//! the same draw lists as WebGPU through the same WGSL (translated by naga,
//! `shader::glsl`), with every GL state change filtered by a cache
//! (`context.rs`). Offscreen targets store rows in WebGPU's order (the shaders flip
//! clip-space Y); the output pass draws straight into the bottom-up canvas and
//! reads the HDR frame flipped.
//!
//! WebGPU features WebGL2 lacks: no storage buffers (instance records live in an
//! RGBA32F texture), no base instance (a uniform per program, `programs.rs`), no
//! depth copies (a depth blit copies the cached fixed-scenery shadow) and no
//! `layout(binding)` (programs bind their blocks and samplers by name).

mod context;
mod programs;
mod resources;
mod textures;

pub use context::{Canvas, Gpu};
pub use programs::Pipelines;
pub use resources::{InstanceStore, MaterialBinding, PageBuffers};
pub use textures::{Sampler, Texture, TextureView, Uploader};

use glow::HasContext;

use super::{
    BLOOM_LEVELS, FrameUniform, INITIAL_SHADOW_BASES, MergedDraw, OcclusionUniform, OutputUniform,
    RenderStats, Scene, WaterUniform, lut,
};
use crate::draw_list::{Draw, Grouping, MAIN_VIEW, REFLECTION_VIEW, SHADOW_VIEW, VIEW_COUNT};
use context::{block, unit};
use resources::SHADOW_BASE_LOCATION;
use textures::{linear_sampler, storage, texel_storage};

/// Frames between `getError` checks: the call waits for the GPU process.
const ERROR_CHECK_FRAMES: u32 = 300;

/// Opaque draws group by material before mesh page: a material binds its uniform
/// block and every texture and sampler that differs, up to a dozen calls into
/// JavaScript, where a page rebinds one vertex array.
pub const DRAW_GROUPING: Grouping = Grouping::MaterialFirst;

/// Report an incomplete framebuffer through the error slot.
fn check_framebuffer(gpu: &Gpu, label: &str) {
    let status = unsafe { gpu.gl.check_framebuffer_status(glow::FRAMEBUFFER) };
    if status != glow::FRAMEBUFFER_COMPLETE {
        gpu.error.set(format!(
            "WebGL canvas unavailable: the {label} framebuffer is incomplete (0x{status:04x})"
        ));
    }
}

/// A framebuffer drawing into level 0 of `texture` at `attachment`, left bound.
fn texture_framebuffer(gpu: &Gpu, attachment: u32, texture: glow::Texture) -> glow::Framebuffer {
    let framebuffer = gpu.created(unsafe { gpu.gl.create_framebuffer() }, "framebuffer");
    gpu.bind_framebuffer(Some(framebuffer));
    unsafe {
        gpu.gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            attachment,
            glow::TEXTURE_2D,
            Some(texture),
            0,
        );
    }
    framebuffer
}

/// A multisampled HDR color and depth target resolved into a texture: the main view
/// (canvas-sized) and the water reflection (fixed size). Dropping it deletes it.
struct ColorTarget {
    gpu: Gpu,
    width: u32,
    height: u32,
    color: glow::Renderbuffer,
    depth: glow::Renderbuffer,
    framebuffer: glow::Framebuffer,
    resolved: glow::Texture,
    /// A single-sample copy of the depth the resolve also blits, for the main view's
    /// ambient occlusion (WebGL2 shaders cannot read multisampled depth).
    resolved_depth: Option<glow::Texture>,
    resolve_framebuffer: glow::Framebuffer,
}

impl ColorTarget {
    fn new(gpu: &Gpu, label: &str, width: u32, height: u32, keep_depth: bool) -> Self {
        let gl = &gpu.gl;
        let renderbuffer = |format| unsafe {
            let renderbuffer = gpu.created(gl.create_renderbuffer(), "renderbuffer");
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(renderbuffer));
            gl.renderbuffer_storage_multisample(
                glow::RENDERBUFFER,
                gpu.samples as i32,
                format,
                width as i32,
                height as i32,
            );
            renderbuffer
        };
        let color = renderbuffer(glow::RGBA16F);
        let depth = renderbuffer(glow::DEPTH_COMPONENT32F);
        let framebuffer = gpu.created(unsafe { gl.create_framebuffer() }, "framebuffer");
        gpu.bind_framebuffer(Some(framebuffer));
        unsafe {
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::RENDERBUFFER,
                Some(color),
            );
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(depth),
            );
        }
        check_framebuffer(gpu, label);
        // The output pass reads it with `texelFetch`.
        let resolved = texel_storage(gpu, glow::RGBA16F, width, height);
        let resolve_framebuffer = texture_framebuffer(gpu, glow::COLOR_ATTACHMENT0, resolved);
        // Read with `texelFetch`: nearest filters keep the depth texture complete.
        let resolved_depth = keep_depth.then(|| {
            let texture = texel_storage(gpu, glow::DEPTH_COMPONENT32F, width, height);
            gpu.bind_framebuffer(Some(resolve_framebuffer));
            unsafe {
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::DEPTH_ATTACHMENT,
                    glow::TEXTURE_2D,
                    Some(texture),
                    0,
                );
            }
            texture
        });
        check_framebuffer(gpu, label);
        Self {
            gpu: gpu.clone(),
            width,
            height,
            color,
            depth,
            framebuffer,
            resolved,
            resolved_depth,
            resolve_framebuffer,
        }
    }

    /// Bind the multisampled framebuffer and clear it to `background`.
    fn begin(&self, gpu: &Gpu, background: [f32; 3]) {
        gpu.bind_framebuffer(Some(self.framebuffer));
        gpu.viewport(self.width, self.height);
        gpu.clear(Some(background));
    }

    /// Resolve into the texture (and, with `depth`, the depth copy), discarding the
    /// multisampled color and depth.
    fn resolve(&self, gpu: &Gpu, depth: bool) {
        let mask = if depth && self.resolved_depth.is_some() {
            glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT
        } else {
            glow::COLOR_BUFFER_BIT
        };
        gpu.blit(
            self.framebuffer,
            self.resolve_framebuffer,
            self.width,
            self.height,
            mask,
        );
        gpu.invalidate_read(&[glow::COLOR_ATTACHMENT0, glow::DEPTH_ATTACHMENT]);
    }
}

impl Drop for ColorTarget {
    fn drop(&mut self) {
        let gl = &self.gpu.gl;
        unsafe {
            gl.delete_framebuffer(self.framebuffer);
            gl.delete_framebuffer(self.resolve_framebuffer);
            gl.delete_renderbuffer(self.color);
            gl.delete_renderbuffer(self.depth);
            gl.delete_texture(self.resolved);
            if let Some(depth) = self.resolved_depth {
                gl.delete_texture(depth);
            }
        }
    }
}

/// The half-size ambient occlusion target and its framebuffer; the output pass
/// samples it through a linear sampler. Dropping it deletes it.
struct OcclusionTarget {
    gpu: Gpu,
    texture: glow::Texture,
    framebuffer: glow::Framebuffer,
    width: u32,
    height: u32,
}

impl OcclusionTarget {
    fn new(gpu: &Gpu, width: u32, height: u32) -> Self {
        let (width, height) = ((width / 2).max(1), (height / 2).max(1));
        let texture = storage(gpu, glow::R8, 1, width, height);
        let framebuffer = texture_framebuffer(gpu, glow::COLOR_ATTACHMENT0, texture);
        check_framebuffer(gpu, "ambient occlusion");
        Self {
            gpu: gpu.clone(),
            texture,
            framebuffer,
            width,
            height,
        }
    }
}

impl Drop for OcclusionTarget {
    fn drop(&mut self) {
        unsafe {
            self.gpu.gl.delete_framebuffer(self.framebuffer);
            self.gpu.gl.delete_texture(self.texture);
        }
    }
}

/// The bloom levels (half the canvas size, then halving), each a linear-filtered
/// RGBA16F texture with its framebuffer. Dropping them deletes them.
struct BloomLevels {
    gpu: Gpu,
    levels: Vec<(glow::Texture, glow::Framebuffer, u32, u32)>,
}

impl BloomLevels {
    fn new(gpu: &Gpu, width: u32, height: u32) -> Self {
        let levels = (1..=BLOOM_LEVELS)
            .map(|level| {
                let (w, h) = ((width >> level).max(1), (height >> level).max(1));
                let texture = storage(gpu, glow::RGBA16F, 1, w, h);
                let framebuffer = texture_framebuffer(gpu, glow::COLOR_ATTACHMENT0, texture);
                check_framebuffer(gpu, "bloom");
                (texture, framebuffer, w, h)
            })
            .collect();
        Self {
            gpu: gpu.clone(),
            levels,
        }
    }

    fn bytes(&self) -> u64 {
        self.levels
            .iter()
            .map(|&(_, _, w, h)| u64::from(w * h) * 8)
            .sum()
    }
}

impl Drop for BloomLevels {
    fn drop(&mut self) {
        let gl = &self.gpu.gl;
        for &(texture, framebuffer, _, _) in &self.levels {
            unsafe {
                gl.delete_framebuffer(framebuffer);
                gl.delete_texture(texture);
            }
        }
    }
}

/// A square depth texture and the framebuffer drawing into it: the sun shadow map
/// and the cached fixed-scenery shadow.
struct DepthTarget {
    gpu: Gpu,
    size: u32,
    texture: glow::Texture,
    framebuffer: glow::Framebuffer,
}

impl DepthTarget {
    fn new(gpu: &Gpu, label: &str, size: u32) -> Self {
        let texture = storage(gpu, glow::DEPTH_COMPONENT32F, 1, size, size);
        let framebuffer = texture_framebuffer(gpu, glow::DEPTH_ATTACHMENT, texture);
        check_framebuffer(gpu, label);
        Self {
            gpu: gpu.clone(),
            size,
            texture,
            framebuffer,
        }
    }

    fn bind(&self, gpu: &Gpu) {
        gpu.bind_framebuffer(Some(self.framebuffer));
        gpu.viewport(self.size, self.size);
    }
}

impl Drop for DepthTarget {
    fn drop(&mut self) {
        unsafe {
            self.gpu.gl.delete_framebuffer(self.framebuffer);
            self.gpu.gl.delete_texture(self.texture);
        }
    }
}

/// WebGPU binds a frame group per instance store; WebGL binds the store's texture
/// as each draw needs it, so there is nothing to build.
pub struct FrameGroups;

/// The main view's target (canvas-sized), the sun's shadow maps and every
/// per-frame binding: view uniforms, instance records and the merged shadows'
/// record bases.
pub struct Frame {
    gpu: Gpu,
    main: ColorTarget,
    bloom: BloomLevels,
    occlusion: OcclusionTarget,
    shadow: DepthTarget,
    /// The cached fixed-scenery shadow, only while frames copy it.
    static_shadow: Option<DepthTarget>,
    lut: glow::Texture,
    shadow_sampler: glow::Sampler,
    /// Linear clamp-to-edge: the DFG LUT and the water's reflection.
    linear_sampler: glow::Sampler,
    view_uniforms: [glow::Buffer; VIEW_COUNT],
    output_uniform: glow::Buffer,
    occlusion_uniform: glow::Buffer,
    instance_records: InstanceStore,
    shadow_bases: glow::Buffer,
    shadow_base_capacity: u32,
    /// Full-screen triangles read no attributes.
    no_attributes: glow::VertexArray,
    /// Signalled once the GPU has run everything before the last [`Self::await_gpu`].
    fence: Option<glow::Fence>,
    frames: u32,
}

impl Frame {
    pub fn new(gpu: &Gpu, canvas: Canvas, shadow_size: u32, instances: u32) -> Self {
        let gl = &gpu.gl;
        let lut = storage(gpu, glow::RG16F, 1, lut::DFG_LUT_SIZE, lut::DFG_LUT_SIZE);
        gpu.set_flip_y(false);
        unsafe {
            gl.tex_sub_image_2d(
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
        let uniform = |bytes| gpu.create_buffer(glow::UNIFORM_BUFFER, bytes, glow::DYNAMIC_DRAW);
        Self {
            gpu: gpu.clone(),
            main: ColorTarget::new(gpu, "main view", canvas.width, canvas.height, true),
            bloom: BloomLevels::new(gpu, canvas.width, canvas.height),
            occlusion: OcclusionTarget::new(gpu, canvas.width, canvas.height),
            shadow: DepthTarget::new(gpu, "sun shadow map", shadow_size),
            static_shadow: None,
            lut,
            shadow_sampler: linear_sampler(gpu, true),
            linear_sampler: linear_sampler(gpu, false),
            view_uniforms: [0, 1, 2].map(|_| uniform(size_of::<FrameUniform>() as u64)),
            output_uniform: uniform(size_of::<OutputUniform>() as u64),
            occlusion_uniform: uniform(size_of::<OcclusionUniform>() as u64),
            instance_records: InstanceStore::new(gpu, "instances", instances),
            shadow_bases: gpu.create_buffer(
                glow::ARRAY_BUFFER,
                u64::from(INITIAL_SHADOW_BASES) * 4,
                glow::DYNAMIC_DRAW,
            ),
            shadow_base_capacity: INITIAL_SHADOW_BASES,
            no_attributes: gpu.created(unsafe { gl.create_vertex_array() }, "vertex array"),
            fence: None,
            frames: 0,
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.main.width, self.main.height)
    }

    /// Resize the canvas drawing buffer and the main view's target.
    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        let width = width.clamp(1, gpu.max_size);
        let height = height.clamp(1, gpu.max_size);
        if (self.main.width, self.main.height) == (width, height) {
            return;
        }
        gpu.canvas.set_width(width);
        gpu.canvas.set_height(height);
        // Replacing the targets deletes the old ones.
        self.main = ColorTarget::new(gpu, "main view", width, height, true);
        self.bloom = BloomLevels::new(gpu, width, height);
        self.occlusion = OcclusionTarget::new(gpu, width, height);
    }

    /// Replace the shadow maps.
    pub fn set_shadow_size(&mut self, gpu: &Gpu, size: u32) {
        if self.static_shadow.is_some() {
            self.static_shadow = Some(DepthTarget::new(gpu, "fixed scenery shadow", size));
        }
        self.shadow = DepthTarget::new(gpu, "sun shadow map", size);
    }

    /// Allocate the fixed-scenery shadow at the sun shadow's size, or free it. Returns
    /// whether it was just allocated, so nothing is drawn in it yet.
    pub fn keep_static_shadow(&mut self, gpu: &Gpu, keep: bool) -> bool {
        if keep == self.static_shadow.is_some() {
            return false;
        }
        self.static_shadow =
            keep.then(|| DepthTarget::new(gpu, "fixed scenery shadow", self.shadow.size));
        keep
    }

    pub fn has_static_shadow(&self) -> bool {
        self.static_shadow.is_some()
    }

    /// The view's instance records.
    pub fn instances(&self) -> &InstanceStore {
        &self.instance_records
    }

    /// Replace the view's instance records with an empty store of `capacity`; the
    /// caller rewrites the static records.
    pub fn grow_instances(&mut self, gpu: &Gpu, capacity: u32) {
        self.instance_records = InstanceStore::new(gpu, "instances", capacity);
    }

    pub fn frame_groups(&self, _gpu: &Gpu, _instances: &InstanceStore) -> FrameGroups {
        FrameGroups
    }

    pub fn write_views(&self, gpu: &Gpu, views: &[FrameUniform; VIEW_COUNT]) {
        for (&buffer, uniform) in self.view_uniforms.iter().zip(views) {
            gpu.write_buffer(buffer, 0, bytemuck::bytes_of(uniform));
        }
    }

    pub fn write_output(&self, gpu: &Gpu, mut uniform: OutputUniform) {
        // The canvas's rows run bottom-up: HDR row = height - y.
        uniform.settings[1] = self.main.height as f32;
        uniform.settings[2] = -1.0;
        // WebGL's weaker devices skip the sharpening's four extra taps per pixel.
        uniform.detail[0] = 0.0;
        gpu.write_buffer(self.output_uniform, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn write_occlusion(&self, gpu: &Gpu, uniform: &OcclusionUniform) {
        gpu.write_buffer(self.occlusion_uniform, 0, bytemuck::bytes_of(uniform));
    }

    /// Bytes of the bloom levels, the occlusion target and the depth copy.
    pub fn post_bytes(&self) -> u64 {
        let occlusion = u64::from(self.occlusion.width * self.occlusion.height);
        let depth = u64::from(self.main.width * self.main.height) * 4;
        self.bloom.bytes() + occlusion + depth
    }

    /// Upload this frame's merged shadow bases.
    pub fn write_shadow_bases(&mut self, gpu: &Gpu, bases: &[u32]) {
        let needed = bases.len() as u32;
        if needed > self.shadow_base_capacity {
            unsafe { gpu.gl.delete_buffer(self.shadow_bases) };
            self.shadow_base_capacity = needed.next_power_of_two();
            self.shadow_bases = gpu.create_buffer(
                glow::ARRAY_BUFFER,
                u64::from(self.shadow_base_capacity) * 4,
                glow::DYNAMIC_DRAW,
            );
        }
        gpu.write_buffer(self.shadow_bases, 0, bytemuck::cast_slice(bases));
    }

    /// Ask to be told when the GPU has run everything submitted so far;
    /// [`Self::gpu_idle`] turns true then.
    pub fn await_gpu(&mut self, gpu: &Gpu) {
        let gl = &gpu.gl;
        unsafe {
            if let Some(fence) = self.fence.take() {
                gl.delete_sync(fence);
            }
            self.fence = gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0).ok();
            gl.flush();
        }
    }

    /// The fence's status updates between the page's tasks, not within one.
    pub fn gpu_idle(&self, gpu: &Gpu) -> bool {
        self.fence
            .is_none_or(|fence| unsafe { gpu.gl.get_sync_status(fence) } == glow::SIGNALED)
    }

    /// Draw a frame into the canvas, which the browser presents once the page yields.
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        scene: &Scene,
        stats: &mut RenderStats,
    ) -> Result<(), String> {
        // Before the frame's draws, so the wait covers the earlier frames and this
        // one's uploads, not the thousands of calls drawing it.
        self.frames += 1;
        if self.frames.is_multiple_of(ERROR_CHECK_FRAMES) {
            gpu.check_error("drawing");
        }
        self.draw_scene(gpu, scene, stats);
        gpu.bind_framebuffer(None);
        gpu.viewport(self.main.width, self.main.height);
        self.draw_output(gpu, scene, stats);
        Ok(())
    }

    /// Draw every pass once, the output into an offscreen probe, so the browser
    /// compiles what each program draws with before gameplay needs it.
    pub fn warm_up(&mut self, gpu: &Gpu, scene: &Scene, stats: &mut RenderStats) {
        self.draw_scene(gpu, scene, stats);
        let probe = storage(gpu, glow::RGBA8, 1, 4, 4);
        let framebuffer = texture_framebuffer(gpu, glow::COLOR_ATTACHMENT0, probe);
        gpu.viewport(4, 4);
        self.draw_output(gpu, scene, stats);
        unsafe {
            gpu.gl.delete_framebuffer(framebuffer);
            gpu.gl.delete_texture(probe);
        }
        gpu.check_error("warming up");
    }

    /// The sun shadow, water reflection and main view passes, into the main target.
    fn draw_scene(&mut self, gpu: &Gpu, scene: &Scene, stats: &mut RenderStats) {
        // Bindings every view shares; the shadow map's is unused by shadow programs.
        gpu.bind_texture(unit::DFG_LUT, self.lut);
        gpu.bind_sampler(unit::DFG_LUT, Some(self.linear_sampler));
        gpu.bind_texture(unit::SHADOW_MAP, self.shadow.texture);
        gpu.bind_sampler(unit::SHADOW_MAP, Some(self.shadow_sampler));
        gpu.bind_texture(unit::INSTANCES, self.instance_records.texture());
        gpu.bind_sampler(unit::INSTANCES, None);
        let draws = DrawContext {
            gpu,
            scene,
            instances: self.instance_records.texture(),
            bases: self.shadow_bases,
        };
        let mut static_count = 0;
        let static_shadow = self.static_shadow.as_ref();
        let static_shadow =
            || static_shadow.expect("frames that copy the fixed scenery shadow keep it");
        if scene.rebuild_static {
            static_shadow().bind(gpu);
            gpu.clear(None);
            gpu.bind_uniform_block(block::FRAME, self.view_uniforms[SHADOW_VIEW]);
            static_count += draws.encode(scene.static_shadow_draws, SHADOW_VIEW, stats);
            static_count += draws.encode_merged(scene.static_merged_draws, stats);
        }
        if scene.copy_static {
            let size = self.shadow.size;
            gpu.blit(
                static_shadow().framebuffer,
                self.shadow.framebuffer,
                size,
                size,
                glow::DEPTH_BUFFER_BIT,
            );
            self.shadow.bind(gpu);
        } else {
            self.shadow.bind(gpu);
            gpu.clear(None);
        }
        gpu.bind_uniform_block(block::FRAME, self.view_uniforms[SHADOW_VIEW]);
        let count = draws.encode(&scene.views[SHADOW_VIEW].opaque, SHADOW_VIEW, stats)
            + draws.encode_merged(scene.merged_draws, stats);
        stats.shadow_draw_calls = count + static_count;
        stats.shadow_triangles = stats.triangles;
        if let (true, Some(water)) = (scene.reflection, scene.water) {
            let target = &water.gpu.target;
            target.begin(gpu, scene.background);
            gpu.bind_uniform_block(block::FRAME, self.view_uniforms[REFLECTION_VIEW]);
            let view = &scene.views[REFLECTION_VIEW];
            let mut count = draws.encode(&view.opaque, REFLECTION_VIEW, stats);
            if scene.sky {
                count += self.draw_sky(gpu, scene, stats);
            }
            count += draws.encode(&view.transparent, REFLECTION_VIEW, stats);
            stats.reflection_draw_calls = count;
            stats.reflection_triangles = stats.triangles - stats.shadow_triangles;
            target.resolve(gpu, false);
        }
        self.main.begin(gpu, scene.background);
        gpu.bind_uniform_block(block::FRAME, self.view_uniforms[MAIN_VIEW]);
        let view = &scene.views[MAIN_VIEW];
        draws.encode(&view.opaque, MAIN_VIEW, stats);
        if scene.sky {
            self.draw_sky(gpu, scene, stats);
        }
        if let Some(water) = scene.water
            && let range = scene.meshes.get(water.mesh).range
            && !range.is_empty()
        {
            let pipeline = &scene.pipelines.fixed().water;
            gpu.use_program(scene.pipelines.program(pipeline.program).raw);
            gpu.set_raster(&pipeline.raster);
            water.gpu.bind(gpu, self.linear_sampler);
            draws.bind_pages(range.vertex_page, range.index_page);
            draws.draw_elements(range.first_index, range.index_count, 1);
            stats.draw_calls += 1;
            stats.triangles += range.index_count as u64 / 3;
        }
        draws.encode(&view.transparent, MAIN_VIEW, stats);
        self.main.resolve(gpu, scene.occlusion);
        if scene.occlusion {
            self.draw_occlusion(gpu, scene, stats);
        }
        if scene.bloom {
            self.draw_bloom(gpu, scene, stats);
        }
    }

    /// Draw the sky where the bound target's depth is still clear; the view's frame
    /// uniform is bound. Returns the draw count.
    fn draw_sky(&self, gpu: &Gpu, scene: &Scene, stats: &mut RenderStats) -> u32 {
        let pipeline = &scene.pipelines.fixed().sky;
        gpu.use_program(scene.pipelines.program(pipeline.program).raw);
        gpu.set_raster(&pipeline.raster);
        gpu.bind_vertex_array(Some(self.no_attributes));
        unsafe { gpu.gl.draw_arrays(glow::TRIANGLES, 0, 3) };
        stats.draw_calls += 1;
        1
    }

    /// The half-size ambient occlusion from the resolved depth copy.
    fn draw_occlusion(&self, gpu: &Gpu, scene: &Scene, stats: &mut RenderStats) {
        let pipeline = &scene.pipelines.fixed().occlusion;
        let Some(depth) = self.main.resolved_depth else {
            return;
        };
        let target = &self.occlusion;
        gpu.bind_framebuffer(Some(target.framebuffer));
        gpu.viewport(target.width, target.height);
        gpu.use_program(scene.pipelines.program(pipeline.program).raw);
        gpu.set_raster(&pipeline.raster);
        gpu.bind_uniform_block(block::OUTPUT, self.occlusion_uniform);
        gpu.bind_texture(unit::SOURCE, depth);
        gpu.bind_sampler(unit::SOURCE, None);
        gpu.bind_vertex_array(Some(self.no_attributes));
        unsafe { gpu.gl.draw_arrays(glow::TRIANGLES, 0, 3) };
        stats.draw_calls += 1;
    }

    /// Threshold the resolved frame into the first bloom level, halve it down the
    /// levels and add each back onto the one above.
    fn draw_bloom(&self, gpu: &Gpu, scene: &Scene, stats: &mut RenderStats) {
        let [prefilter, down, up] = &scene.pipelines.fixed().bloom;
        let levels = &self.bloom.levels;
        gpu.bind_uniform_block(block::OUTPUT, self.output_uniform);
        gpu.bind_sampler(unit::SOURCE, Some(self.linear_sampler));
        gpu.bind_vertex_array(Some(self.no_attributes));
        let pass = |pipeline: &programs::Pipeline, source: glow::Texture, target: usize| {
            let (_, framebuffer, width, height) = levels[target];
            gpu.bind_framebuffer(Some(framebuffer));
            gpu.viewport(width, height);
            gpu.use_program(scene.pipelines.program(pipeline.program).raw);
            gpu.set_raster(&pipeline.raster);
            gpu.bind_texture(unit::SOURCE, source);
            unsafe { gpu.gl.draw_arrays(glow::TRIANGLES, 0, 3) };
        };
        pass(prefilter, self.main.resolved, 0);
        for level in 1..BLOOM_LEVELS {
            pass(down, levels[level - 1].0, level);
        }
        for level in (0..BLOOM_LEVELS - 1).rev() {
            pass(up, levels[level + 1].0, level);
        }
        stats.draw_calls += 2 * BLOOM_LEVELS as u32 - 1;
    }

    /// Draw the resolved main view into the bound framebuffer through the output
    /// transform.
    fn draw_output(&self, gpu: &Gpu, scene: &Scene, stats: &mut RenderStats) {
        let pipeline = &scene.pipelines.fixed().output;
        gpu.use_program(scene.pipelines.program(pipeline.program).raw);
        gpu.set_raster(&pipeline.raster);
        gpu.bind_uniform_block(block::OUTPUT, self.output_uniform);
        gpu.bind_texture(unit::SOURCE, self.main.resolved);
        gpu.bind_sampler(unit::SOURCE, None);
        gpu.bind_texture(unit::BLOOM, self.bloom.levels[0].0);
        gpu.bind_sampler(unit::BLOOM, Some(self.linear_sampler));
        // Read only while the occlusion is on, which also draws it.
        gpu.bind_texture(unit::OCCLUSION, self.occlusion.texture);
        gpu.bind_sampler(unit::OCCLUSION, Some(self.linear_sampler));
        gpu.bind_vertex_array(Some(self.no_attributes));
        unsafe { gpu.gl.draw_arrays(glow::TRIANGLES, 0, 3) };
        stats.draw_calls += 1;
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        let gl = &self.gpu.gl;
        unsafe {
            gl.delete_texture(self.lut);
            gl.delete_buffer(self.occlusion_uniform);
            for sampler in [self.shadow_sampler, self.linear_sampler] {
                gl.delete_sampler(sampler);
            }
            for buffer in self.view_uniforms {
                gl.delete_buffer(buffer);
            }
            gl.delete_buffer(self.output_uniform);
            gl.delete_buffer(self.shadow_bases);
            gl.delete_vertex_array(self.no_attributes);
            if let Some(fence) = self.fence {
                gl.delete_sync(fence);
            }
        }
    }
}

/// The water's uniform, reflection target and normal map. Dropping it deletes the
/// uniform and the target.
pub struct WaterGpu {
    gpu: Gpu,
    uniform: glow::Buffer,
    target: ColorTarget,
    normals: glow::Texture,
    normals_sampler: glow::Sampler,
}

impl WaterGpu {
    pub fn new(
        gpu: &Gpu,
        _frame: &Frame,
        size: u32,
        normals: &TextureView,
        sampler: Sampler,
    ) -> Self {
        Self {
            gpu: gpu.clone(),
            uniform: gpu.create_buffer(
                glow::UNIFORM_BUFFER,
                size_of::<WaterUniform>() as u64,
                glow::DYNAMIC_DRAW,
            ),
            target: ColorTarget::new(gpu, "water reflection", size, size, false),
            normals: *normals,
            normals_sampler: sampler,
        }
    }

    /// Bind another normal map (the real one, once it has loaded).
    pub fn rebind(&mut self, _gpu: &Gpu, _frame: &Frame, normals: &TextureView, sampler: Sampler) {
        self.normals = *normals;
        self.normals_sampler = sampler;
    }

    pub fn write(&self, gpu: &Gpu, uniform: &WaterUniform) {
        gpu.write_buffer(self.uniform, 0, bytemuck::bytes_of(uniform));
    }

    /// Bind the uniform, normal map and reflection (group 1) for the water draw.
    fn bind(&self, gpu: &Gpu, reflection_sampler: glow::Sampler) {
        gpu.bind_uniform_block(block::MATERIAL, self.uniform);
        gpu.bind_texture(unit::MATERIAL, self.normals);
        gpu.bind_sampler(unit::MATERIAL, Some(self.normals_sampler));
        gpu.bind_texture(unit::MATERIAL + 1, self.target.resolved);
        gpu.bind_sampler(unit::MATERIAL + 1, Some(reflection_sampler));
    }
}

impl Drop for WaterGpu {
    fn drop(&mut self) {
        unsafe { self.gpu.gl.delete_buffer(self.uniform) };
    }
}

/// Draws a pass's lists; every binding goes through the state cache.
struct DrawContext<'a> {
    gpu: &'a Gpu,
    scene: &'a Scene<'a>,
    /// The view's instance records; pool draws bind their own and put these back.
    instances: glow::Texture,
    bases: glow::Buffer,
}

impl DrawContext<'_> {
    fn bind_pages(&self, vertex_page: u16, index_page: u16) {
        let (vertices, _) = self.scene.meshes.page(vertex_page);
        let (indices, _) = self.scene.meshes.page(index_page);
        vertices.bind(self.gpu, indices);
    }

    fn draw_elements(&self, first_index: u32, count: u32, instances: u32) {
        unsafe {
            self.gpu.gl.draw_elements_instanced(
                glow::TRIANGLES,
                count as i32,
                glow::UNSIGNED_INT,
                first_index as i32 * 4,
                instances as i32,
            );
        }
    }

    fn encode_merged(&self, draws: &[MergedDraw], stats: &mut RenderStats) -> u32 {
        let scene = self.scene;
        let gpu = self.gpu;
        let mut count = 0;
        for draw in draws {
            let Some(mesh) = scene
                .models
                .at(draw.model)
                .and_then(|model| model.shadow.get(draw.group as usize))
                .filter(|mesh| !mesh.range.is_empty())
            else {
                continue;
            };
            let pipeline = &scene.pipelines.fixed().shadow_merged[mesh.pipeline];
            gpu.use_program(scene.pipelines.program(pipeline.program).raw);
            gpu.set_raster(&pipeline.raster);
            if let Some(material) = mesh.material {
                scene.materials.get(material).binding.bind(gpu);
            }
            let range = mesh.range;
            self.bind_pages(range.vertex_page, range.index_page);
            // WebGL2 has no base instance: point the bases at this draw's first one.
            gpu.bind_array_buffer(self.bases);
            unsafe {
                gpu.gl.vertex_attrib_pointer_i32(
                    SHADOW_BASE_LOCATION,
                    1,
                    glow::UNSIGNED_INT,
                    4,
                    draw.first as i32 * 4,
                );
            }
            self.draw_elements(range.first_index, range.index_count, draw.count);
            count += 1;
            stats.draw_calls += 1;
            stats.triangles += (range.index_count / 3) as u64 * draw.count as u64;
        }
        count
    }

    /// Draw `draws` for `view`. Returns the draw count. Consecutive draws mostly
    /// bind the same state (`draw_list::DrawState` orders them so), so only a
    /// change of it binds anything.
    fn encode(&self, draws: &[Draw], view: usize, stats: &mut RenderStats) -> u32 {
        let scene = self.scene;
        let gpu = self.gpu;
        let shadow = view == SHADOW_VIEW;
        let mut bound: Option<DrawBinding> = None;
        let mut count = 0;
        for draw in draws {
            let Some(class) = scene.classes.at(draw.class) else {
                continue;
            };
            // An empty mesh has no page to bind and nothing to draw.
            let range = scene.meshes.get(class.key.mesh).range;
            if range.is_empty() {
                continue;
            }
            let instances = match class.key.pool {
                Some(pool) => match scene.pools.at(pool) {
                    Some(entry) => entry.records.texture(),
                    None => continue,
                },
                None => self.instances,
            };
            let Some(pipeline) = (if shadow { class.shadow } else { class.main }) else {
                continue;
            };
            let back = if shadow { None } else { class.back };
            for pipeline in back.into_iter().chain([pipeline]) {
                let program = scene
                    .pipelines
                    .program(scene.pipelines.get(pipeline).program);
                let binding = DrawBinding {
                    pipeline,
                    material: program.reads_material.then_some(class.key.material),
                    vertex_page: range.vertex_page,
                    index_page: range.index_page,
                    instances,
                };
                if bound != Some(binding) {
                    self.bind(bound, binding);
                    bound = Some(binding);
                }
                program.set_first_instance(gpu, draw.first_instance);
                self.draw_elements(range.first_index, range.index_count, draw.instance_count);
                count += 1;
                stats.triangles += (range.index_count / 3) as u64 * draw.instance_count as u64;
            }
        }
        gpu.bind_texture(unit::INSTANCES, self.instances);
        stats.draw_calls += count;
        count
    }

    /// Bind what `next` changes from `last` (`None`: everything).
    fn bind(&self, last: Option<DrawBinding>, next: DrawBinding) {
        let gpu = self.gpu;
        let scene = self.scene;
        let changed = |same: fn(&DrawBinding, &DrawBinding) -> bool| {
            last.is_none_or(|last| !same(&last, &next))
        };
        if changed(|a, b| a.pipeline == b.pipeline) {
            let pipeline = scene.pipelines.get(next.pipeline);
            gpu.use_program(scene.pipelines.program(pipeline.program).raw);
            gpu.set_raster(&pipeline.raster);
        }
        if let Some(material) = next.material
            && changed(|a, b| a.material == b.material)
        {
            scene.materials.get(material).binding.bind(gpu);
        }
        if changed(|a, b| (a.vertex_page, a.index_page) == (b.vertex_page, b.index_page)) {
            self.bind_pages(next.vertex_page, next.index_page);
        }
        if changed(|a, b| a.instances == b.instances) {
            gpu.bind_texture(unit::INSTANCES, next.instances);
        }
    }
}

/// The state an opaque or transparent draw binds; draws in a row that share it
/// differ only in mesh range and instances.
#[derive(Clone, Copy, PartialEq)]
struct DrawBinding {
    pipeline: u32,
    /// `None` for a program that reads no material (most shadow casters).
    material: Option<u32>,
    vertex_page: u16,
    index_page: u16,
    instances: glow::Texture,
}
