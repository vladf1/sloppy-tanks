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

/// What the renderer draws into: the page's canvas.
pub type RenderTarget = web_sys::HtmlCanvasElement;
pub use programs::Pipelines;
pub use resources::{InstanceStore, MaterialBinding, PageBuffers};
pub use textures::{Sampler, Texture, TextureView, Uploader};

use glow::HasContext;

use super::{
    BLOOM_FIRST_SHIFT, BLOOM_LEVELS, BloomPass, FrameUniform, INITIAL_SHADOW_BASES, MergedDraw,
    OCCLUSION_DOWNSCALE, OcclusionUniform, OutputUniform, RenderStats, Scene, WaterUniform, lut,
};
use crate::atmosphere_map::{ATMOSPHERE_MAP_SIZE, atmosphere_map};
use crate::draw_list::{Draw, Grouping, MAIN_VIEW, REFLECTION_VIEW, SHADOW_VIEW, VIEW_COUNT};
use crate::gpu::blurred_occlusion_size;
use crate::noise_lattice::{NOISE_LATTICE_SIZE, noise_lattice};
use crate::target_memory::target_bytes;
use context::{block, unit};
use resources::SHADOW_BASE_LOCATION;
use textures::{linear_sampler, storage, texel_storage};

/// Ambient occlusion reads the depth copy the main view's resolve blits; WebGL2 has
/// no depth pass of its own for it.
pub const OCCLUSION_DEPTH_PASS: bool = false;

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
/// (the scene's size) and the water reflection (fixed size). Dropping it deletes it.
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
        // The output pass reads it with `texelFetch`, or through the linear sampler
        // where it upsamples a smaller scene.
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

/// The quarter-size ambient occlusion target and its blur at each corner of its
/// texels (`blurred_occlusion_size`), each with its framebuffer; the blur
/// samples the occlusion and the output pass the blur, both through a linear
/// sampler. Dropping it deletes them.
struct OcclusionTarget {
    gpu: Gpu,
    texture: glow::Texture,
    framebuffer: glow::Framebuffer,
    width: u32,
    height: u32,
    blurred: glow::Texture,
    blurred_framebuffer: glow::Framebuffer,
}

impl OcclusionTarget {
    fn new(gpu: &Gpu, width: u32, height: u32) -> Self {
        let (width, height) = (
            (width / OCCLUSION_DOWNSCALE).max(1),
            (height / OCCLUSION_DOWNSCALE).max(1),
        );
        let texture = storage(gpu, glow::R8, 1, width, height);
        let framebuffer = texture_framebuffer(gpu, glow::COLOR_ATTACHMENT0, texture);
        check_framebuffer(gpu, "ambient occlusion");
        let (blurred_width, blurred_height) = blurred_occlusion_size(width, height);
        let blurred = storage(gpu, glow::R8, 1, blurred_width, blurred_height);
        let blurred_framebuffer = texture_framebuffer(gpu, glow::COLOR_ATTACHMENT0, blurred);
        check_framebuffer(gpu, "ambient occlusion blur");
        Self {
            gpu: gpu.clone(),
            texture,
            framebuffer,
            width,
            height,
            blurred,
            blurred_framebuffer,
        }
    }
}

impl Drop for OcclusionTarget {
    fn drop(&mut self) {
        unsafe {
            self.gpu.gl.delete_framebuffer(self.framebuffer);
            self.gpu.gl.delete_texture(self.texture);
            self.gpu.gl.delete_framebuffer(self.blurred_framebuffer);
            self.gpu.gl.delete_texture(self.blurred);
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
        let levels = (BLOOM_FIRST_SHIFT..BLOOM_FIRST_SHIFT + BLOOM_LEVELS)
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

/// The dialog backdrop's two targets, the canvas's size: the output pass's picture and
/// its blur along the rows, each an RGBA8 texture with its framebuffer. Their rows run
/// bottom-up like the canvas's, since the output pass draws into the first as into the
/// canvas. Made while a dialog blurs the arena, dropped with the blur or a new canvas
/// size; dropping deletes them.
struct BackdropTargets {
    gpu: Gpu,
    size: (u32, u32),
    picture: (glow::Texture, glow::Framebuffer),
    rows: (glow::Texture, glow::Framebuffer),
}

impl BackdropTargets {
    fn new(gpu: &Gpu, size: (u32, u32)) -> Self {
        let target = |label| {
            let texture = storage(gpu, glow::RGBA8, 1, size.0, size.1);
            let framebuffer = texture_framebuffer(gpu, glow::COLOR_ATTACHMENT0, texture);
            check_framebuffer(gpu, label);
            (texture, framebuffer)
        };
        Self {
            gpu: gpu.clone(),
            size,
            picture: target("backdrop picture"),
            rows: target("backdrop rows"),
        }
    }

    fn bytes(&self) -> u64 {
        2 * u64::from(self.size.0 * self.size.1) * 4
    }
}

impl Drop for BackdropTargets {
    fn drop(&mut self) {
        for (texture, framebuffer) in [self.picture, self.rows] {
            unsafe {
                self.gpu.gl.delete_framebuffer(framebuffer);
                self.gpu.gl.delete_texture(texture);
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

/// The main view's target (the scene's size), the sun's shadow maps and every
/// per-frame binding: view uniforms, instance records and the merged shadows'
/// record bases.
pub struct Frame {
    gpu: Gpu,
    /// The canvas drawing buffer's width and height.
    canvas: (u32, u32),
    main: ColorTarget,
    bloom: BloomLevels,
    occlusion: OcclusionTarget,
    shadow: DepthTarget,
    /// The cached fixed-scenery shadow, only while frames copy it.
    static_shadow: Option<DepthTarget>,
    lut: glow::Texture,
    /// The noise lattice the frame's shaders fetch (`noise_lattice.rs`) and the
    /// atmosphere map they sample (`atmosphere_map.rs`).
    noise: glow::Texture,
    atmosphere_map: glow::Texture,
    shadow_sampler: glow::Sampler,
    /// Linear clamp-to-edge: the DFG LUT and the water's reflection.
    linear_sampler: glow::Sampler,
    view_uniforms: [glow::Buffer; VIEW_COUNT],
    output_uniform: glow::Buffer,
    occlusion_uniform: glow::Buffer,
    /// The output uniform last written blurs the arena behind a dialog, and the
    /// targets that blur draws through while it does.
    backdrop_on: bool,
    backdrop: Option<BackdropTargets>,
    instance_records: InstanceStore,
    shadow_bases: glow::Buffer,
    shadow_base_capacity: u32,
    /// Full-screen triangles read no attributes.
    no_attributes: glow::VertexArray,
    /// Signalled once the GPU has run everything before the last [`Self::await_gpu`].
    fence: Option<glow::Fence>,
    frames: u32,
    /// The canvas frame waits for [`Frame::present`], as the WebGPU backend's does
    /// (`Renderer::present_late`).
    pub late_present: bool,
    /// The main view's target still holds the last drawn scene, which a canvas frame
    /// without a new one shows again.
    scene_drawn: bool,
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
        let upload = |texture, size: u32, data: &[u8]| {
            gpu.bind_upload_texture(texture);
            unsafe {
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    0,
                    0,
                    size as i32,
                    size as i32,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(data)),
                );
            }
        };
        let noise = texel_storage(gpu, glow::RGBA8, NOISE_LATTICE_SIZE, NOISE_LATTICE_SIZE);
        upload(noise, NOISE_LATTICE_SIZE, &noise_lattice());
        let atmosphere = storage(
            gpu,
            glow::RGBA8,
            1,
            ATMOSPHERE_MAP_SIZE,
            ATMOSPHERE_MAP_SIZE,
        );
        upload(atmosphere, ATMOSPHERE_MAP_SIZE, &atmosphere_map());
        let uniform = |bytes| gpu.create_buffer(glow::UNIFORM_BUFFER, bytes, glow::DYNAMIC_DRAW);
        Self {
            gpu: gpu.clone(),
            canvas: (canvas.width, canvas.height),
            main: ColorTarget::new(gpu, "main view", canvas.width, canvas.height, true),
            bloom: BloomLevels::new(gpu, canvas.width, canvas.height),
            occlusion: OcclusionTarget::new(gpu, canvas.width, canvas.height),
            shadow: DepthTarget::new(gpu, "sun shadow map", shadow_size),
            static_shadow: None,
            lut,
            noise,
            atmosphere_map: atmosphere,
            shadow_sampler: linear_sampler(gpu, true),
            linear_sampler: linear_sampler(gpu, false),
            view_uniforms: [0, 1, 2].map(|_| uniform(size_of::<FrameUniform>() as u64)),
            output_uniform: uniform(size_of::<OutputUniform>() as u64),
            occlusion_uniform: uniform(size_of::<OcclusionUniform>() as u64),
            backdrop_on: false,
            backdrop: None,
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
            late_present: false,
            scene_drawn: false,
        }
    }

    /// The canvas drawing buffer's size.
    pub fn size(&self) -> (u32, u32) {
        self.canvas
    }

    /// The main view's size, which the output pass upsamples to the canvas's.
    pub fn scene_size(&self) -> (u32, u32) {
        (self.main.width, self.main.height)
    }

    /// Resize the canvas drawing buffer to `canvas` and the main view's target, with
    /// its post-processing targets, to `scene`.
    pub fn resize(&mut self, gpu: &Gpu, canvas: (u32, u32), scene: (u32, u32)) {
        let fit = |(width, height): (u32, u32)| {
            (width.clamp(1, gpu.max_size), height.clamp(1, gpu.max_size))
        };
        let (canvas, scene) = (fit(canvas), fit(scene));
        if self.canvas != canvas {
            gpu.canvas.set_width(canvas.0);
            gpu.canvas.set_height(canvas.1);
            self.canvas = canvas;
            self.backdrop = None;
        }
        if self.scene_size() == scene {
            return;
        }
        let (width, height) = scene;
        // Replacing the targets deletes the old ones.
        self.scene_drawn = false;
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

    /// The main view's frame uniform, which bakes read (`effects::bakes`).
    pub fn main_uniform(&self) -> glow::Buffer {
        self.view_uniforms[MAIN_VIEW]
    }

    pub fn write_views(&self, gpu: &Gpu, views: &[FrameUniform; VIEW_COUNT]) {
        for (&buffer, uniform) in self.view_uniforms.iter().zip(views) {
            gpu.write_buffer(buffer, 0, bytemuck::bytes_of(uniform));
        }
    }

    pub fn write_output(&mut self, gpu: &Gpu, mut uniform: OutputUniform) {
        // The canvas's rows run bottom-up: top-down row = height - y.
        uniform.settings[1] = self.canvas.1 as f32;
        uniform.settings[2] = -1.0;
        // WebGL's weaker devices skip the sharpening's four extra taps per pixel.
        uniform.detail[0] = 0.0;
        gpu.write_buffer(self.output_uniform, 0, bytemuck::bytes_of(&uniform));
        self.backdrop_on = uniform.backdrop[0] > 0.0;
        if !self.backdrop_on {
            self.backdrop = None;
        }
    }

    pub fn write_occlusion(&self, gpu: &Gpu, uniform: &OcclusionUniform) {
        gpu.write_buffer(self.occlusion_uniform, 0, bytemuck::bytes_of(uniform));
    }

    /// Bytes of the main view's target, the bloom levels, the occlusion target, the
    /// depth copy and the shadow maps.
    pub fn target_bytes(&self) -> u64 {
        let main = target_bytes(self.main.width, self.main.height, self.gpu.samples);
        let (width, height) = (self.occlusion.width, self.occlusion.height);
        let (blurred_width, blurred_height) = blurred_occlusion_size(width, height);
        let occlusion = u64::from(width * height + blurred_width * blurred_height);
        let depth = u64::from(self.main.width * self.main.height) * 4;
        let shadow = [Some(&self.shadow), self.static_shadow.as_ref()]
            .into_iter()
            .flatten()
            .map(|map| u64::from(map.size) * u64::from(map.size) * 4)
            .sum::<u64>();
        let backdrop = self.backdrop.as_ref().map_or(0, BackdropTargets::bytes);
        main + self.bloom.bytes() + occlusion + depth + shadow + backdrop
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

    /// Nothing to reset: the WebGL build samples no GPU time, so it is never asked.
    pub fn reset_canvas_pacing(&mut self, _gpu: &Gpu) {}

    /// WebGL offers no completion time finer than a frame (its fences update between
    /// the page's tasks), so no GPU frame time is sampled.
    pub fn take_gpu_frame_ms(&self) -> Option<f64> {
        None
    }

    /// The fence's status updates between the page's tasks, not within one.
    pub fn gpu_idle(&self, gpu: &Gpu) -> bool {
        self.fence
            .is_none_or(|fence| unsafe { gpu.gl.get_sync_status(fence) } == glow::SIGNALED)
    }

    /// WebGL's own swap holds animation frames back while the GPU is behind.
    pub fn frames_in_flight(&self) -> u32 {
        0
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
        self.scene_drawn = true;
        // Under the shell's overlay the canvas waits for `present`.
        if self.late_present {
            return Ok(());
        }
        self.draw_canvas(gpu, scene.pipelines, stats);
        Ok(())
    }

    /// Under `late_present`, a canvas frame: the scene [`Self::draw`] drew last through
    /// the output pass (before any, the canvas cleared to `clear`) with `overlay` (the
    /// shell's menus and HUD) drawn over it at the canvas's size, after which the context
    /// is the backend's again. The browser shows the canvas once the page yields.
    pub fn present(
        &mut self,
        gpu: &Gpu,
        pipelines: &Pipelines,
        clear: [f32; 3],
        overlay: impl FnOnce((u32, u32)),
    ) {
        if self.scene_drawn {
            // The output pass's draws are not counted: the frame's stats were taken.
            self.draw_canvas(gpu, pipelines, &mut RenderStats::default());
        } else {
            gpu.bind_framebuffer(None);
            gpu.viewport(self.canvas.0, self.canvas.1);
            gpu.clear(Some(clear));
        }
        // The overlay uploads its own textures, unflipped; the backend's bitmaps leave
        // uploads flipped.
        gpu.set_flip_y(false);
        overlay(self.canvas);
        gpu.reclaim();
    }

    /// Draw every pass once, the output into an offscreen probe, so the browser
    /// compiles what each program draws with before gameplay needs it.
    pub fn warm_up(&mut self, gpu: &Gpu, scene: &Scene, stats: &mut RenderStats) {
        self.draw_scene(gpu, scene, stats);
        let probe = storage(gpu, glow::RGBA8, 1, 4, 4);
        let framebuffer = texture_framebuffer(gpu, glow::COLOR_ATTACHMENT0, probe);
        gpu.viewport(4, 4);
        self.draw_output(gpu, scene.pipelines, stats);
        // The dialog backdrop's blur, from the scene's frame: the program has drawn into
        // the canvas's format before the first dialog opens.
        self.draw_backdrop_pass(
            gpu,
            scene.pipelines,
            BloomPass::BackdropColumns,
            self.main.resolved,
            Some(framebuffer),
        );
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
        gpu.bind_texture(unit::NOISE, self.noise);
        gpu.bind_sampler(unit::NOISE, None);
        gpu.bind_texture(unit::ATMOSPHERE_MAP, self.atmosphere_map);
        gpu.bind_sampler(unit::ATMOSPHERE_MAP, Some(self.linear_sampler));
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
            let pipeline = scene.pipelines.water();
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

    /// The quarter-size ambient occlusion from the resolved depth copy.
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
        let blur = &scene.pipelines.fixed().occlusion_blur;
        let program = scene.pipelines.program(blur.program);
        gpu.bind_framebuffer(Some(target.blurred_framebuffer));
        let (blurred_width, blurred_height) = blurred_occlusion_size(target.width, target.height);
        gpu.viewport(blurred_width, blurred_height);
        gpu.use_program(program.raw);
        program.set_first_instance(gpu, BloomPass::OcclusionBlur as u32);
        gpu.set_raster(&blur.raster);
        // The bloom program's other passes read the output block: it must be bound.
        gpu.bind_uniform_block(block::OUTPUT, self.output_uniform);
        gpu.bind_texture(unit::SOURCE, target.texture);
        gpu.bind_sampler(unit::SOURCE, Some(self.linear_sampler));
        unsafe { gpu.gl.draw_arrays(glow::TRIANGLES, 0, 3) };
        stats.draw_calls += 2;
    }

    /// Threshold the resolved frame into the first bloom level, halve it down the
    /// levels and add each back onto the one above.
    fn draw_bloom(&self, gpu: &Gpu, scene: &Scene, stats: &mut RenderStats) {
        let levels = &self.bloom.levels;
        gpu.bind_uniform_block(block::OUTPUT, self.output_uniform);
        gpu.bind_sampler(unit::SOURCE, Some(self.linear_sampler));
        gpu.bind_vertex_array(Some(self.no_attributes));
        let pass = |kind: BloomPass, source: glow::Texture, target: usize| {
            let pipeline = &scene.pipelines.fixed().bloom[kind as usize];
            let program = scene.pipelines.program(pipeline.program);
            let (_, framebuffer, width, height) = levels[target];
            gpu.bind_framebuffer(Some(framebuffer));
            gpu.viewport(width, height);
            gpu.use_program(program.raw);
            program.set_first_instance(gpu, kind as u32);
            gpu.set_raster(&pipeline.raster);
            gpu.bind_texture(unit::SOURCE, source);
            unsafe { gpu.gl.draw_arrays(glow::TRIANGLES, 0, 3) };
        };
        pass(BloomPass::Prefilter, self.main.resolved, 0);
        for level in 1..BLOOM_LEVELS {
            pass(BloomPass::Down, levels[level - 1].0, level);
        }
        for level in (0..BLOOM_LEVELS - 1).rev() {
            pass(BloomPass::Up, levels[level + 1].0, level);
        }
        stats.draw_calls += 2 * BLOOM_LEVELS as u32 - 1;
    }

    /// The output pass into the canvas, left bound at its size. Behind a dialog it draws
    /// into the backdrop's picture instead, which the blur along the rows reads and the
    /// blur down the columns draws into the canvas from.
    fn draw_canvas(&mut self, gpu: &Gpu, pipelines: &Pipelines, stats: &mut RenderStats) {
        let (width, height) = self.canvas;
        if !self.backdrop_on {
            gpu.bind_framebuffer(None);
            gpu.viewport(width, height);
            self.draw_output(gpu, pipelines, stats);
            return;
        }
        if self
            .backdrop
            .as_ref()
            .is_none_or(|targets| targets.size != self.canvas)
        {
            self.backdrop = Some(BackdropTargets::new(gpu, self.canvas));
        }
        let targets = self.backdrop.as_ref().expect("made above");
        let (picture, rows) = (targets.picture, targets.rows);
        gpu.bind_framebuffer(Some(picture.1));
        gpu.viewport(width, height);
        self.draw_output(gpu, pipelines, stats);
        self.draw_backdrop_pass(
            gpu,
            pipelines,
            BloomPass::BackdropRows,
            picture.0,
            Some(rows.1),
        );
        self.draw_backdrop_pass(gpu, pipelines, BloomPass::BackdropColumns, rows.0, None);
        stats.draw_calls += 2;
    }

    /// One of the dialog backdrop's blur passes from `source` into `framebuffer`, at the
    /// viewport bound.
    fn draw_backdrop_pass(
        &self,
        gpu: &Gpu,
        pipelines: &Pipelines,
        pass: BloomPass,
        source: glow::Texture,
        framebuffer: Option<glow::Framebuffer>,
    ) {
        let blur = &pipelines.fixed().backdrop;
        let program = pipelines.program(blur.program);
        gpu.bind_framebuffer(framebuffer);
        gpu.use_program(program.raw);
        program.set_first_instance(gpu, pass as u32);
        gpu.set_raster(&blur.raster);
        gpu.bind_uniform_block(block::OUTPUT, self.output_uniform);
        gpu.bind_texture(unit::SOURCE, source);
        gpu.bind_sampler(unit::SOURCE, Some(self.linear_sampler));
        gpu.bind_vertex_array(Some(self.no_attributes));
        unsafe { gpu.gl.draw_arrays(glow::TRIANGLES, 0, 3) };
    }

    /// Draw the resolved main view into the bound framebuffer through the output
    /// transform.
    fn draw_output(&self, gpu: &Gpu, pipelines: &Pipelines, stats: &mut RenderStats) {
        let pipeline = &pipelines.fixed().output;
        gpu.use_program(pipelines.program(pipeline.program).raw);
        gpu.set_raster(&pipeline.raster);
        gpu.bind_uniform_block(block::OUTPUT, self.output_uniform);
        // The main view's uniform, for the haze the output adds.
        gpu.bind_uniform_block(block::FRAME, self.view_uniforms[MAIN_VIEW]);
        gpu.bind_texture(unit::SOURCE, self.main.resolved);
        // `texelFetch` at full scale ignores it; a smaller scene upsamples through it.
        gpu.bind_sampler(unit::SOURCE, Some(self.linear_sampler));
        gpu.bind_texture(unit::BLOOM, self.bloom.levels[0].0);
        gpu.bind_sampler(unit::BLOOM, Some(self.linear_sampler));
        // Read only while the occlusion is on, which also draws it.
        gpu.bind_texture(unit::OCCLUSION, self.occlusion.blurred);
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
            gl.delete_texture(self.noise);
            gl.delete_texture(self.atmosphere_map);
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

    /// Bytes the reflection target holds.
    pub fn target_bytes(&self) -> u64 {
        target_bytes(self.target.width, self.target.height, self.gpu.samples)
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
                let indices = draw.indices(range.indices());
                let index_count = indices.end - indices.start;
                self.draw_elements(indices.start, index_count, draw.instance_count);
                count += 1;
                stats.triangles += (index_count / 3) as u64 * draw.instance_count as u64;
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
