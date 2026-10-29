//! The WebGPU backend: wgpu on the browser's WebGPU. Its frame encodes the sun
//! shadow, water reflection, main view, ambient occlusion (its depth, then the
//! occlusion) and bloom passes into one command buffer and the output pass into
//! another.

mod context;
mod instance_store;
mod pipelines;
mod precompile;
mod resources;
mod textures;

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_arch = "wasm32")]
use std::{cell::RefCell, rc::Rc};
#[cfg(not(target_arch = "wasm32"))]
use std::{sync::Mutex, time::Instant};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};
use wgpu::util::DeviceExt;

pub use context::{Canvas, Gpu, RenderTarget};
pub use instance_store::InstanceStore;
pub use pipelines::Pipelines;
pub use resources::{MaterialBinding, PageBuffers};
pub use textures::{Sampler, Texture, TextureView, Uploader};

use super::{
    BLOOM_FIRST_SHIFT, BLOOM_LEVELS, BloomPass, FrameUniform, INITIAL_SHADOW_BASES, MergedDraw,
    OCCLUSION_DOWNSCALE, OcclusionUniform, OutputUniform, RenderStats, Scene, WaterUniform, lut,
};
use crate::atmosphere_map::{ATMOSPHERE_MAP_SIZE, atmosphere_map};
use crate::draw_list::{Draw, Grouping, MAIN_VIEW, REFLECTION_VIEW, SHADOW_VIEW, VIEW_COUNT};
use crate::gpu::blurred_occlusion_size;
use crate::mesh_pages::{MeshRange, NO_PAGE};
use crate::noise_lattice::{NOISE_LATTICE_SIZE, noise_lattice};
use context::{ColorTarget, DEPTH_FORMAT, OCCLUSION_FORMAT, SHADOW_DEPTH_FORMAT, texture_2d};
use resources::uniform_buffer;

/// Opaque draws group by mesh page before material. Every switch is a call Chrome
/// validates in the GPU process, and a page switch costs one or two vertex buffers
/// and often the index buffer where a material is one bind group: grouping by page
/// first cut the GPU process 11-20% against class index order (material first,
/// 6-18%), and neither changed the main thread measurably.
pub const DRAW_GROUPING: Grouping = Grouping::PageFirst;

/// Ambient occlusion reads a depth of its own: a depth-only pass of the main view's
/// opaque draws at `OCCLUSION_DEPTH_DOWNSCALE`. WebGPU cannot resolve a multisampled
/// depth, and reading the main view's would store all four samples every frame
/// (16 bytes a pixel) and keep them in memory, where they now never leave tile memory.
pub const OCCLUSION_DEPTH_PASS: bool = true;
/// The occlusion depth is the main view's size divided by this. Half size keeps the
/// normals occlusion rebuilds and the depth its probes test close to the full-size
/// depth's, for a quarter of the pixels.
const OCCLUSION_DEPTH_DOWNSCALE: u32 = 2;

/// A square shadow map, which frames copy between (the cached fixed scenery).
fn depth_texture(device: &wgpu::Device, label: &str, size: u32) -> wgpu::Texture {
    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST;
    texture_2d(device, label, size, size, SHADOW_DEPTH_FORMAT, 1, usage)
}

/// A depth texture with its view; dropping it destroys the texture.
struct DepthMap {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl DepthMap {
    fn new(device: &wgpu::Device, label: &str, size: u32) -> Self {
        Self::of(depth_texture(device, label, size))
    }

    fn of(texture: wgpu::Texture) -> Self {
        Self {
            view: texture.create_view(&Default::default()),
            texture,
        }
    }
}

impl Drop for DepthMap {
    fn drop(&mut self) {
        self.texture.destroy();
    }
}

/// Canvas frames on the GPU, which one completion callback per frame tracks: how many
/// are still running, and how long the GPU took to finish the latest, from its scene's
/// submission until the queue reported everything submitted done (the GPU's own time
/// when it was idle at the submission, and more when it was still busy).
#[derive(Default)]
struct GpuFrames {
    /// When each canvas frame whose GPU work has not finished had its scene submitted
    /// (ms), oldest first: the queue reports work done in submission order.
    submitted: VecDeque<f64>,
    /// The latest finished frame's milliseconds, until taken.
    fresh: Option<f64>,
}

#[cfg(target_arch = "wasm32")]
/// The completion callbacks of [`GpuFrames`]: one pair of closures serves every frame,
/// each frame only chaining them onto its `onSubmittedWorkDone` promise. wgpu's own
/// `on_submitted_work_done` spawns a task with a future and two new promise closures
/// per call, about 50 µs of main-thread time a frame in Chrome.
struct FrameTracker {
    frames: Rc<RefCell<GpuFrames>>,
    performance: Option<web_sys::Performance>,
    /// Both are there until the tracker drops.
    done: Option<Closure<dyn FnMut(JsValue)>>,
    /// A lost device rejects the promise; the frame still leaves the count.
    failed: Option<Closure<dyn FnMut(JsValue)>>,
}

#[cfg(target_arch = "wasm32")]
impl FrameTracker {
    fn new() -> Self {
        let frames = Rc::new(RefCell::new(GpuFrames::default()));
        let performance = web_sys::window().and_then(|window| window.performance());
        let finish = {
            let frames = frames.clone();
            let performance = performance.clone();
            move || {
                let mut frames = frames.borrow_mut();
                if let Some(submitted) = frames.submitted.pop_front() {
                    let now = performance.as_ref().map_or(0.0, web_sys::Performance::now);
                    frames.fresh = Some(now - submitted);
                }
            }
        };
        let on_failure = finish.clone();
        Self {
            frames,
            performance,
            done: Some(Closure::new(move |_: JsValue| finish())),
            failed: Some(Closure::new(move |_: JsValue| on_failure())),
        }
    }

    fn now_ms(&self) -> f64 {
        self.performance
            .as_ref()
            .map_or(0.0, web_sys::Performance::now)
    }

    fn take_fresh(&self) -> Option<f64> {
        self.frames.borrow_mut().fresh.take()
    }

    fn in_flight(&self) -> u32 {
        self.frames.borrow().submitted.len() as u32
    }
}

/// [`GpuFrames`] natively. wgpu runs a frame's completion callback only from a device
/// poll; one made by the next frame would time a frame as one display frame whatever
/// its work, and Auto's quality would never find the room to step up again. So a
/// thread waits on the GPU and runs each callback as its work finishes.
#[cfg(not(target_arch = "wasm32"))]
struct FrameTracker {
    frames: Arc<Mutex<GpuFrames>>,
    start: Instant,
    /// Ends the waiting thread, which holds a handle to the device.
    stop: Arc<AtomicBool>,
}

/// How long the waiting thread waits at a time, and rests when nothing is on the GPU.
#[cfg(not(target_arch = "wasm32"))]
const GPU_WAIT: std::time::Duration = std::time::Duration::from_millis(100);
#[cfg(not(target_arch = "wasm32"))]
const GPU_IDLE_REST: std::time::Duration = std::time::Duration::from_millis(1);

#[cfg(not(target_arch = "wasm32"))]
impl FrameTracker {
    fn new(device: &wgpu::Device) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let (device, stopped) = (device.clone(), stop.clone());
        let _ = std::thread::Builder::new()
            .name("gpu completion".into())
            .spawn(move || {
                while !stopped.load(Ordering::Acquire) {
                    let waited = device.poll(wgpu::PollType::Wait {
                        submission_index: None,
                        timeout: Some(GPU_WAIT),
                    });
                    if waited.is_ok_and(|status| status.is_queue_empty()) {
                        std::thread::sleep(GPU_IDLE_REST);
                    }
                }
            });
        Self {
            frames: Arc::default(),
            start: Instant::now(),
            stop,
        }
    }

    fn now_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn lock(frames: &Mutex<GpuFrames>) -> std::sync::MutexGuard<'_, GpuFrames> {
        frames
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn take_fresh(&self) -> Option<f64> {
        Self::lock(&self.frames).fresh.take()
    }

    fn in_flight(&self) -> u32 {
        Self::lock(&self.frames).submitted.len() as u32
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for FrameTracker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for FrameTracker {
    /// Frames still on the GPU settle their promises after the tracker is gone, and a
    /// dropped closure would throw when they do: the two closures are left to them.
    fn drop(&mut self) {
        if let Some(done) = self.done.take() {
            done.forget();
        }
        if let Some(failed) = self.failed.take() {
            failed.forget();
        }
    }
}

fn base_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shadow bases"),
        size: capacity as u64 * 4,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// The frame bind group of each view with one instance store as its `instances`: the
/// view's own, or an effect pool's.
pub struct FrameGroups([wgpu::BindGroup; VIEW_COUNT]);

/// The canvas, the main view's target, the sun's shadow maps and every per-frame
/// binding: view uniforms, instance records and the merged shadows' record bases.
pub struct Frame {
    canvas: Canvas,
    main_target: ColorTarget,
    shadow_map: DepthMap,
    /// The cached fixed-scenery shadow, only while frames copy it.
    static_shadow_map: Option<DepthMap>,
    dummy_depth: wgpu::TextureView,
    shadow_sampler: wgpu::Sampler,
    /// Nearest clamp-to-edge: gathers of the shadow map's depths.
    shadow_texel_sampler: wgpu::Sampler,
    lut_view: wgpu::TextureView,
    /// The noise lattice the frame's shaders fetch (`noise_lattice.rs`) and the
    /// atmosphere map they sample (`atmosphere_map.rs`).
    noise_view: wgpu::TextureView,
    atmosphere_map_view: wgpu::TextureView,
    /// Linear clamp-to-edge: the DFG LUT and the water's reflection.
    linear_sampler: wgpu::Sampler,
    view_uniforms: [wgpu::Buffer; VIEW_COUNT],
    view_groups: FrameGroups,
    instance_records: InstanceStore,
    output_uniform: wgpu::Buffer,
    occlusion_uniform: wgpu::Buffer,
    post: PostTargets,
    shadow_base_buffer: wgpu::Buffer,
    shadow_base_capacity: u32,
    /// Set once the GPU has run everything submitted before the last
    /// [`Frame::await_gpu`].
    gpu_idle: Arc<AtomicBool>,
    gpu_frames: FrameTracker,
    /// Natively, whether the window took the last frame: a covered window gives none,
    /// and nothing paces frames to its display.
    pub shown: bool,
    /// Natively, the main view's targets still hold the last drawn scene, which a
    /// window frame without a new one shows again (as a canvas keeps its picture).
    scene_drawn: bool,
    /// In the browser, the canvas frame waits for [`Frame::present`] as a window's does
    /// natively (`Renderer::present_late`).
    #[cfg(target_arch = "wasm32")]
    pub late_present: bool,
}

impl Frame {
    pub fn new(gpu: &Gpu, canvas: Canvas, shadow_size: u32, instances: u32) -> Self {
        let device = &gpu.device;
        let (width, height) = (canvas.config.width, canvas.config.height);
        let lut = device.create_texture_with_data(
            &gpu.queue,
            &wgpu::TextureDescriptor {
                label: Some("DFG LUT"),
                size: wgpu::Extent3d {
                    width: lut::DFG_LUT_SIZE,
                    height: lut::DFG_LUT_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rg16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            bytemuck::cast_slice(&lut::DFG_LUT),
        );
        let rgba8 = |label, size, data: &[u8]| {
            device.create_texture_with_data(
                &gpu.queue,
                &wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size,
                        height: size,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                data,
            )
        };
        let noise = rgba8("noise lattice", NOISE_LATTICE_SIZE, &noise_lattice());
        let atmosphere = rgba8("atmosphere map", ATMOSPHERE_MAP_SIZE, &atmosphere_map());
        let main_target = ColorTarget::new(gpu, "main view", width, height);
        let output_uniform = uniform_buffer(device, "output", size_of::<OutputUniform>() as u64);
        let occlusion_uniform =
            uniform_buffer(device, "occlusion", size_of::<OcclusionUniform>() as u64);
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let view_uniforms =
            [0, 1, 2].map(|_| uniform_buffer(device, "frame", size_of::<FrameUniform>() as u64));
        let post = PostTargets::new(
            gpu,
            &main_target,
            &view_uniforms[MAIN_VIEW],
            &output_uniform,
            &occlusion_uniform,
            &linear_sampler,
        );
        let instance_records = InstanceStore::new(gpu, "instances", instances);
        let mut frame = Self {
            canvas,
            main_target,
            shadow_map: DepthMap::new(device, "sun shadow map", shadow_size),
            static_shadow_map: None,
            dummy_depth: depth_texture(device, "shadow pass placeholder", 1)
                .create_view(&Default::default()),
            shadow_sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("shadow compare"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                compare: Some(wgpu::CompareFunction::LessEqual),
                ..Default::default()
            }),
            shadow_texel_sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("shadow texels"),
                ..Default::default()
            }),
            lut_view: lut.create_view(&Default::default()),
            noise_view: noise.create_view(&Default::default()),
            atmosphere_map_view: atmosphere.create_view(&Default::default()),
            linear_sampler,
            view_uniforms,
            // Replaced below, once the frame's own resources exist.
            view_groups: FrameGroups(std::array::from_fn(|_| post.output_group.clone())),
            instance_records,
            output_uniform,
            occlusion_uniform,
            post,
            shadow_base_buffer: base_buffer(device, INITIAL_SHADOW_BASES),
            shadow_base_capacity: INITIAL_SHADOW_BASES,
            gpu_idle: Arc::new(AtomicBool::new(true)),
            #[cfg(target_arch = "wasm32")]
            gpu_frames: FrameTracker::new(),
            #[cfg(not(target_arch = "wasm32"))]
            gpu_frames: FrameTracker::new(device),
            shown: true,
            scene_drawn: false,
            #[cfg(target_arch = "wasm32")]
            late_present: false,
        };
        frame.view_groups = frame.frame_groups(gpu, &frame.instance_records);
        frame
    }

    /// The canvas drawing buffer's size.
    pub fn size(&self) -> (u32, u32) {
        (self.canvas.config.width, self.canvas.config.height)
    }

    /// The main view's size, which the output pass upsamples to the canvas's.
    pub fn scene_size(&self) -> (u32, u32) {
        let size = self.main_target.resolved.size();
        (size.width, size.height)
    }

    /// Resize the canvas drawing buffer to `canvas` and the main view's target, with
    /// its post-processing targets, to `scene`.
    pub fn resize(&mut self, gpu: &Gpu, canvas: (u32, u32), scene: (u32, u32)) {
        let limit = gpu.device.limits().max_texture_dimension_2d;
        let fit = |(width, height): (u32, u32)| (width.clamp(1, limit), height.clamp(1, limit));
        let (canvas, scene) = (fit(canvas), fit(scene));
        let config = &mut self.canvas.config;
        if (config.width, config.height) != canvas {
            (config.width, config.height) = canvas;
            self.canvas.surface.configure(&gpu.device, config);
        }
        if self.scene_size() == scene {
            return;
        }
        let (width, height) = scene;
        self.scene_drawn = false;
        self.main_target = ColorTarget::new(gpu, "main view", width, height);
        self.post = PostTargets::new(
            gpu,
            &self.main_target,
            &self.view_uniforms[MAIN_VIEW],
            &self.output_uniform,
            &self.occlusion_uniform,
            &self.linear_sampler,
        );
    }

    /// Replace the shadow maps; the caller rebuilds the pools' frame groups.
    pub fn set_shadow_size(&mut self, gpu: &Gpu, size: u32) {
        if self.static_shadow_map.is_some() {
            self.static_shadow_map = Some(DepthMap::new(&gpu.device, "fixed scenery shadow", size));
        }
        self.shadow_map = DepthMap::new(&gpu.device, "sun shadow map", size);
        self.view_groups = self.frame_groups(gpu, &self.instance_records);
    }

    /// Allocate the fixed-scenery shadow at the sun shadow's size, or free it. Returns
    /// whether it was just allocated, so nothing is drawn in it yet.
    pub fn keep_static_shadow(&mut self, gpu: &Gpu, keep: bool) -> bool {
        if keep == self.static_shadow_map.is_some() {
            return false;
        }
        self.static_shadow_map = keep.then(|| {
            let size = self.shadow_map.texture.width();
            DepthMap::new(&gpu.device, "fixed scenery shadow", size)
        });
        keep
    }

    /// The view's instance records.
    pub fn instances(&self) -> &InstanceStore {
        &self.instance_records
    }

    /// The main view's frame group, which bakes read (`effects::bakes`).
    pub fn main_group(&self) -> &wgpu::BindGroup {
        &self.view_groups.0[MAIN_VIEW]
    }

    /// Replace the view's instance records with an empty store of `capacity`; the
    /// caller rewrites the static records and rebuilds the pools' frame groups.
    pub fn grow_instances(&mut self, gpu: &Gpu, capacity: u32) {
        self.instance_records = InstanceStore::new(gpu, "instances", capacity);
        self.view_groups = self.frame_groups(gpu, &self.instance_records);
    }

    /// The frame bind group of every view with `instances` as its instance records.
    pub fn frame_groups(&self, gpu: &Gpu, instances: &InstanceStore) -> FrameGroups {
        FrameGroups(std::array::from_fn(|view| {
            let shadow = if view == SHADOW_VIEW {
                &self.dummy_depth
            } else {
                &self.shadow_map.view
            };
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("frame"),
                layout: &gpu.layouts.frame,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.view_uniforms[view].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(shadow),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&self.lut_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: instances.binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: wgpu::BindingResource::TextureView(&self.noise_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 11,
                        resource: wgpu::BindingResource::TextureView(&self.atmosphere_map_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 12,
                        resource: wgpu::BindingResource::Sampler(&self.shadow_texel_sampler),
                    },
                ],
            })
        }))
    }

    pub fn write_views(&self, gpu: &Gpu, views: &[FrameUniform; VIEW_COUNT]) {
        for (buffer, uniform) in self.view_uniforms.iter().zip(views) {
            gpu.queue
                .write_buffer(buffer, 0, bytemuck::bytes_of(uniform));
        }
    }

    pub fn write_output(&self, gpu: &Gpu, mut uniform: OutputUniform) {
        // The canvas's rows run top-down like the HDR target's: row = 0 + 1 × y.
        uniform.settings[1] = 0.0;
        uniform.settings[2] = 1.0;
        gpu.queue
            .write_buffer(&self.output_uniform, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn write_occlusion(&self, gpu: &Gpu, uniform: &OcclusionUniform) {
        gpu.queue
            .write_buffer(&self.occlusion_uniform, 0, bytemuck::bytes_of(uniform));
    }

    /// Bytes the main view's target, the post-processing targets and the shadow maps
    /// hold in memory.
    pub fn target_bytes(&self) -> u64 {
        let shadow = [Some(&self.shadow_map), self.static_shadow_map.as_ref()]
            .into_iter()
            .flatten()
            .map(|map| context::texture_bytes(&map.texture))
            .sum::<u64>();
        self.main_target.bytes() + self.post.bytes + shadow
    }

    /// Upload this frame's merged shadow bases.
    pub fn write_shadow_bases(&mut self, gpu: &Gpu, bases: &[u32]) {
        let needed = bases.len() as u32;
        if needed > self.shadow_base_capacity {
            self.shadow_base_buffer.destroy();
            self.shadow_base_capacity = needed.next_power_of_two();
            self.shadow_base_buffer = base_buffer(&gpu.device, self.shadow_base_capacity);
        }
        if needed > 0 {
            gpu.queue
                .write_buffer(&self.shadow_base_buffer, 0, bytemuck::cast_slice(bases));
        }
    }

    /// Ask to be told when the GPU has run everything submitted so far;
    /// [`Self::gpu_idle`] turns true then.
    pub fn await_gpu(&mut self, gpu: &Gpu) {
        let idle = Arc::new(AtomicBool::new(false));
        self.gpu_idle = idle.clone();
        gpu.queue
            .on_submitted_work_done(move || idle.store(true, Ordering::Release));
    }

    /// WebGPU runs wgpu's callbacks from the browser's event loop; natively a poll
    /// runs those due.
    pub fn gpu_idle(&self, gpu: &Gpu) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = gpu.device.poll(wgpu::PollType::Poll);
        #[cfg(target_arch = "wasm32")]
        let _ = gpu;
        self.gpu_idle.load(Ordering::Acquire)
    }

    /// Configure the canvas again as it is. WebKit lowers a WebGPU canvas's frame rate
    /// when a frame's GPU work overruns a display frame and never raises it again on
    /// its own; a configuration starts its pacing afresh.
    pub fn reset_canvas_pacing(&mut self, gpu: &Gpu) {
        self.canvas
            .surface
            .configure(&gpu.device, &self.canvas.config);
    }

    /// The GPU time of the latest finished frame in milliseconds, once: `None` until a
    /// new sample arrives.
    pub fn take_gpu_frame_ms(&self) -> Option<f64> {
        self.gpu_frames.take_fresh()
    }

    /// Canvas frames the GPU has not finished. Chrome holds animation frames back
    /// while the GPU is behind; WebKit lets them queue.
    pub fn frames_in_flight(&self) -> u32 {
        self.gpu_frames.in_flight()
    }

    /// Count a canvas frame whose scene was submitted at `submitted` (ms) as in flight
    /// until the GPU finishes it, and then take its GPU time.
    #[cfg(target_arch = "wasm32")]
    fn track_frame(&self, gpu: &Gpu, submitted: f64) {
        let tracker = &self.gpu_frames;
        let (Some(queue), Some(on_done), Some(on_failure)) =
            (gpu.queue.as_webgpu(), &tracker.done, &tracker.failed)
        else {
            return;
        };
        tracker.frames.borrow_mut().submitted.push_back(submitted);
        let done: js_sys::Promise = queue.on_submitted_work_done().unchecked_into();
        // The promise `then` returns is dropped: nothing waits on it.
        let _ = done.then2(on_done, on_failure);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn track_frame(&self, gpu: &Gpu, submitted: f64) {
        let frames = self.gpu_frames.frames.clone();
        let start = self.gpu_frames.start;
        FrameTracker::lock(&frames).submitted.push_back(submitted);
        gpu.queue.on_submitted_work_done(move || {
            let mut frames = FrameTracker::lock(&frames);
            if let Some(submitted) = frames.submitted.pop_front() {
                frames.fresh = Some(start.elapsed().as_secs_f64() * 1000.0 - submitted);
            }
        });
    }

    /// Natively (and in the browser under `late_present`), show a window frame: the
    /// scene [`Self::draw`] drew last through the
    /// output pass (or, before any scene, the window cleared to `clear`), with
    /// `overlay` (the window's menus and HUD) drawn over it with the device, the queue
    /// and the frame's view. The window's frame is taken only here, as late as can be:
    /// with the display's pacing that wait would otherwise count as the engine's render
    /// time, and a frame drawn twice (preparation's first frames) shows once. Without a
    /// new scene (a room waiting for the server) the last one shows again, as a canvas
    /// keeps its picture.
    pub fn present(
        &mut self,
        gpu: &Gpu,
        pipelines: &Pipelines,
        clear: wgpu::Color,
        overlay: impl FnOnce(&wgpu::Device, &wgpu::Queue, &wgpu::TextureView),
    ) {
        let _ = gpu.device.poll(wgpu::PollType::Poll);
        let output = match self.canvas.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.canvas
                    .surface
                    .configure(&gpu.device, &self.canvas.config);
                return;
            }
            // Covered: nothing shows, and nothing paces frames to the display.
            wgpu::CurrentSurfaceTexture::Occluded => {
                self.shown = false;
                return;
            }
            // A slow display: try again next frame.
            wgpu::CurrentSurfaceTexture::Timeout => return,
            error => {
                gpu.error.set(format!("Window unavailable: {error:?}"));
                return;
            }
        };
        let view = output.texture.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        if self.scene_drawn {
            // The output pass's draw is not counted: the frame's stats were taken.
            self.encode_output(&mut encoder, &view, pipelines, &mut RenderStats::default());
        } else {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blank window"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        gpu.queue.submit([encoder.finish()]);
        overlay(&gpu.device, &gpu.queue, &view);
        gpu.queue.present(output);
        self.shown = true;
    }

    /// Draw a frame into the canvas. The scene and the canvas go in separate command
    /// buffers. WebKit paces a WebGPU canvas by the GPU time of the command buffers
    /// that write its texture (`WebGPUFramePacer`) and lowers the frame rate when that
    /// exceeds a display frame; with the whole frame in one buffer the scene counted
    /// against the canvas and Safari settled at 30 fps. The output buffer is one
    /// full-screen triangle.
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        scene: &Scene,
        stats: &mut RenderStats,
    ) -> Result<(), String> {
        let device = &gpu.device;
        #[cfg(not(target_arch = "wasm32"))]
        let _ = device.poll(wgpu::PollType::Poll);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("scene"),
        });
        self.encode_scene(&mut encoder, scene, stats);
        let submitted = self.gpu_frames.now_ms();
        gpu.queue.submit([encoder.finish()]);
        // Natively, and under the browser's overlay shell, the window's frame waits for
        // `present`.
        #[cfg(target_arch = "wasm32")]
        if !self.late_present {
            return self.draw_canvas(gpu, scene, stats, submitted);
        }
        self.scene_drawn = true;
        self.track_frame(gpu, submitted);
        Ok(())
    }

    /// The browser's canvas frame: the output pass into it, then present.
    #[cfg(target_arch = "wasm32")]
    fn draw_canvas(
        &mut self,
        gpu: &Gpu,
        scene: &Scene,
        stats: &mut RenderStats,
        submitted: f64,
    ) -> Result<(), String> {
        let device = &gpu.device;
        let output = match self.canvas.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.canvas.surface.configure(device, &self.canvas.config);
                return Ok(());
            }
            error => return Err(format!("Canvas unavailable: {error:?}. Reload to restart.")),
        };
        let view = output.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("output"),
        });
        self.encode_output(&mut encoder, &view, scene.pipelines, stats);
        gpu.queue.submit([encoder.finish()]);
        gpu.queue.present(output);
        self.track_frame(gpu, submitted);
        Ok(())
    }

    /// Draw every pass once, the output into an offscreen probe, so the browser
    /// finishes compiling before gameplay needs the pipelines.
    pub fn warm_up(&mut self, gpu: &Gpu, scene: &Scene, stats: &mut RenderStats) {
        let device = &gpu.device;
        let probe = texture_2d(
            device,
            "warm-up output",
            4,
            4,
            self.canvas.config.format,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let probe_view = probe.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("warm-up"),
        });
        self.encode_scene(&mut encoder, scene, stats);
        self.encode_output(&mut encoder, &probe_view, scene.pipelines, stats);
        gpu.queue.submit([encoder.finish()]);
        probe.destroy();
    }

    /// The sun shadow, water reflection and main view passes, into `main_target`.
    fn encode_scene(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        stats: &mut RenderStats,
    ) {
        let background = scene.background;
        let clear = wgpu::Color {
            r: background[0] as f64,
            g: background[1] as f64,
            b: background[2] as f64,
            a: 1.0,
        };
        let draws = DrawContext {
            scene,
            frame_groups: &self.view_groups,
        };
        let mut static_count = 0;
        let static_shadow = || {
            self.static_shadow_map
                .as_ref()
                .expect("frames that copy the fixed scenery shadow keep it")
        };
        if scene.rebuild_static {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fixed scenery shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &static_shadow().view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.view_groups.0[SHADOW_VIEW], &[]);
            static_count += draws.encode(
                &mut pass,
                scene.static_shadow_draws,
                SHADOW_VIEW,
                Shadow,
                stats,
            );
            static_count += draws.encode_merged(
                &mut pass,
                scene.static_merged_draws,
                &self.shadow_base_buffer,
                stats,
            );
        }
        if scene.copy_static {
            // A depth-only format: copying all aspects copies its depth.
            encoder.copy_texture_to_texture(
                static_shadow().texture.as_image_copy(),
                self.shadow_map.texture.as_image_copy(),
                self.shadow_map.texture.size(),
            );
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_map.view,
                    depth_ops: Some(wgpu::Operations {
                        load: if scene.copy_static {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(1.0)
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.view_groups.0[SHADOW_VIEW], &[]);
            let mut count = draws.encode(
                &mut pass,
                &scene.views[SHADOW_VIEW].opaque,
                SHADOW_VIEW,
                Shadow,
                stats,
            );
            count += draws.encode_merged(
                &mut pass,
                scene.merged_draws,
                &self.shadow_base_buffer,
                stats,
            );
            stats.shadow_draw_calls = count + static_count;
            stats.shadow_triangles = stats.triangles;
        }
        if let (true, Some(water)) = (scene.reflection, scene.water) {
            let target = &water.gpu.target;
            let mut pass = scene_pass(encoder, "water reflection", target, clear);
            pass.set_bind_group(0, &self.view_groups.0[REFLECTION_VIEW], &[]);
            let view = &scene.views[REFLECTION_VIEW];
            let mut count = draws.encode(&mut pass, &view.opaque, REFLECTION_VIEW, Lit, stats);
            if scene.sky {
                count += draw_sky(&mut pass, scene, stats);
            }
            count += draws.encode(&mut pass, &view.transparent, REFLECTION_VIEW, Lit, stats);
            stats.reflection_draw_calls = count;
            stats.reflection_triangles = stats.triangles - stats.shadow_triangles;
        }
        // The occlusion reads its own depth pass, not the main view: it goes first, so
        // its short dependent passes run beside the main view's work rather than after.
        let fixed = scene.pipelines.fixed();
        if scene.occlusion {
            self.encode_occlusion_depth(encoder, &draws, stats);
            let pipeline = &fixed.occlusion;
            let post = &self.post;
            post_draw(
                encoder,
                "ambient occlusion",
                pipeline,
                &post.occlusion_group,
                &post.occlusion_view,
                false,
                0,
            );
            post_draw(
                encoder,
                "ambient occlusion blur",
                &fixed.occlusion_blur,
                &post.occlusion_blur_group,
                &post.occlusion_blurred.1,
                false,
                BloomPass::OcclusionBlur as u32,
            );
            stats.draw_calls += 2;
        }
        {
            let target = &self.main_target;
            let mut pass = scene_pass(encoder, "main view", target, clear);
            pass.set_bind_group(0, &self.view_groups.0[MAIN_VIEW], &[]);
            let view = &scene.views[MAIN_VIEW];
            draws.encode(&mut pass, &view.opaque, MAIN_VIEW, Lit, stats);
            if scene.sky {
                draw_sky(&mut pass, scene, stats);
            }
            if let Some(water) = scene.water
                && let range = scene.meshes.get(water.mesh).range
                && !range.is_empty()
            {
                pass.set_pipeline(scene.pipelines.water());
                pass.set_bind_group(1, &water.gpu.bind_group, &[]);
                BoundPages::default().bind(&mut pass, scene.meshes, range);
                pass.draw_indexed(range.indices(), 0, 0..1);
                stats.draw_calls += 1;
                stats.triangles += range.index_count as u64 / 3;
            }
            draws.encode(&mut pass, &view.transparent, MAIN_VIEW, Lit, stats);
        }
        if scene.bloom {
            let post = &self.post;
            let [prefilter, down, up] = &fixed.bloom;
            post_draw(
                encoder,
                "bloom",
                prefilter,
                &post.frame_group,
                &post.bloom[0].1,
                false,
                BloomPass::Prefilter as u32,
            );
            for level in 1..BLOOM_LEVELS {
                post_draw(
                    encoder,
                    "bloom",
                    down,
                    &post.bloom_groups[level - 1],
                    &post.bloom[level].1,
                    false,
                    BloomPass::Down as u32,
                );
            }
            for level in (0..BLOOM_LEVELS - 1).rev() {
                post_draw(
                    encoder,
                    "bloom",
                    up,
                    &post.bloom_groups[level + 1],
                    &post.bloom[level].1,
                    true,
                    BloomPass::Up as u32,
                );
            }
            stats.draw_calls += 2 * BLOOM_LEVELS as u32 - 1;
        }
    }

    /// The main view's opaque draws and the water surface, depth only, into the
    /// occlusion depth: the main view's frame group and instance records, at
    /// `OCCLUSION_DEPTH_DOWNSCALE`.
    fn encode_occlusion_depth(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        draws: &DrawContext,
        stats: &mut RenderStats,
    ) {
        let scene = draws.scene;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("occlusion depth"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.post.occlusion_depth.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let (draw_calls, triangles) = (stats.draw_calls, stats.triangles);
        pass.set_bind_group(0, &self.view_groups.0[MAIN_VIEW], &[]);
        draws.encode(
            &mut pass,
            &scene.views[MAIN_VIEW].opaque,
            MAIN_VIEW,
            Depth,
            stats,
        );
        if let Some(water) = scene.water
            && let range = scene.meshes.get(water.mesh).range
            && !range.is_empty()
        {
            pass.set_pipeline(scene.pipelines.water_depth());
            pass.set_bind_group(1, &water.gpu.bind_group, &[]);
            BoundPages::default().bind(&mut pass, scene.meshes, range);
            pass.draw_indexed(range.indices(), 0, 0..1);
            stats.draw_calls += 1;
            stats.triangles += range.index_count as u64 / 3;
        }
        stats.occlusion_draw_calls = stats.draw_calls - draw_calls;
        stats.occlusion_triangles = stats.triangles - triangles;
    }

    /// Draw `main_target` into `output` through the output transform.
    fn encode_output(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        pipelines: &Pipelines,
        stats: &mut RenderStats,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("output"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipelines.fixed().output);
        pass.set_bind_group(0, &self.post.output_group, &[]);
        pass.draw(0..3, 0..1);
        stats.draw_calls += 1;
    }
}

/// Draw the sky into a scene pass whose frame group is bound; returns the draw count.
fn draw_sky(pass: &mut wgpu::RenderPass, scene: &Scene, stats: &mut RenderStats) -> u32 {
    pass.set_pipeline(&scene.pipelines.fixed().sky);
    pass.draw(0..3, 0..1);
    stats.draw_calls += 1;
    1
}

/// One full-screen post draw into `target`, replacing it or (`add`) blending onto it.
/// A full-screen pass into `target`, adding onto what it holds if `add`, drawn as
/// instance `instance` (the bloom chain's `BloomPass`).
fn post_draw(
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    target: &wgpu::TextureView,
    add: bool,
    instance: u32,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: if add {
                    wgpu::LoadOp::Load
                } else {
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                },
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, group, &[]);
    pass.draw(0..3, instance..instance + 1);
}

/// The post-processing targets sized from the main view and the bind groups of every
/// pass that reads it: the bloom levels (quarter size, then halving), the quarter-size
/// ambient occlusion with its depth and the output. Rebuilt with the main target;
/// dropping it destroys the textures.
struct PostTargets {
    bloom: Vec<(wgpu::Texture, wgpu::TextureView)>,
    /// Bind groups reading each bloom level, and the resolved frame.
    bloom_groups: Vec<wgpu::BindGroup>,
    frame_group: wgpu::BindGroup,
    occlusion: wgpu::Texture,
    occlusion_view: wgpu::TextureView,
    occlusion_depth: DepthMap,
    occlusion_group: wgpu::BindGroup,
    /// The occlusion's blur at each corner of its texels, which the output reads,
    /// and the blur pass's bind group (the occlusion as its source).
    occlusion_blurred: (wgpu::Texture, wgpu::TextureView),
    occlusion_blur_group: wgpu::BindGroup,
    output_group: wgpu::BindGroup,
    bytes: u64,
}

impl PostTargets {
    fn new(
        gpu: &Gpu,
        main: &ColorTarget,
        main_view: &wgpu::Buffer,
        output_uniform: &wgpu::Buffer,
        occlusion_uniform: &wgpu::Buffer,
        sampler: &wgpu::Sampler,
    ) -> Self {
        let device = &gpu.device;
        let size = main.resolved.size();
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let target = |label, format, width: u32, height: u32| {
            let texture = texture_2d(device, label, width.max(1), height.max(1), format, 1, usage);
            let view = texture.create_view(&Default::default());
            (texture, view)
        };
        let bloom: Vec<_> = (BLOOM_FIRST_SHIFT..BLOOM_FIRST_SHIFT + BLOOM_LEVELS)
            .map(|level| {
                target(
                    "bloom",
                    gpu.bloom_format,
                    size.width >> level,
                    size.height >> level,
                )
            })
            .collect();
        let (occlusion, occlusion_view) = target(
            "ambient occlusion",
            OCCLUSION_FORMAT,
            size.width / OCCLUSION_DOWNSCALE,
            size.height / OCCLUSION_DOWNSCALE,
        );
        let (blurred_width, blurred_height) = blurred_occlusion_size(
            size.width / OCCLUSION_DOWNSCALE,
            size.height / OCCLUSION_DOWNSCALE,
        );
        let occlusion_blurred = target(
            "ambient occlusion blur",
            OCCLUSION_FORMAT,
            blurred_width,
            blurred_height,
        );
        let occlusion_depth = DepthMap::of(texture_2d(
            device,
            "occlusion depth",
            (size.width / OCCLUSION_DEPTH_DOWNSCALE).max(1),
            (size.height / OCCLUSION_DEPTH_DOWNSCALE).max(1),
            DEPTH_FORMAT,
            1,
            usage,
        ));
        let bloom_group = |view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom"),
                layout: &gpu.layouts.bloom,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: output_uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
        };
        let bloom_groups = bloom.iter().map(|(_, view)| bloom_group(view)).collect();
        let frame_group = bloom_group(&main.resolved_view);
        let occlusion_blur_group = bloom_group(&occlusion_view);
        let occlusion_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ambient occlusion"),
            layout: &gpu.layouts.occlusion,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&occlusion_depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: occlusion_uniform.as_entire_binding(),
                },
            ],
        });
        let output_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("output"),
            layout: &gpu.layouts.output,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&main.resolved_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&bloom[0].1),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&occlusion_blurred.1),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: main_view.as_entire_binding(),
                },
            ],
        });
        let bytes = bloom
            .iter()
            .map(|(texture, _)| context::texture_bytes(texture))
            .sum::<u64>()
            + context::texture_bytes(&occlusion)
            + context::texture_bytes(&occlusion_blurred.0)
            + context::texture_bytes(&occlusion_depth.texture);
        Self {
            bloom,
            bloom_groups,
            frame_group,
            occlusion,
            occlusion_view,
            occlusion_depth,
            occlusion_group,
            occlusion_blurred,
            occlusion_blur_group,
            output_group,
            bytes,
        }
    }
}

impl Drop for PostTargets {
    fn drop(&mut self) {
        for (texture, _) in &self.bloom {
            texture.destroy();
        }
        self.occlusion.destroy();
        self.occlusion_blurred.0.destroy();
    }
}

/// The water's uniform, reflection target and bind group (uniform, normals,
/// reflection). Dropping it destroys the uniform and the target.
pub struct WaterGpu {
    uniform: wgpu::Buffer,
    target: ColorTarget,
    bind_group: wgpu::BindGroup,
}

impl WaterGpu {
    pub fn new(
        gpu: &Gpu,
        frame: &Frame,
        size: u32,
        normals: &TextureView,
        sampler: Sampler,
    ) -> Self {
        let uniform = uniform_buffer(&gpu.device, "water", size_of::<WaterUniform>() as u64);
        let target = ColorTarget::new(gpu, "water reflection", size, size);
        let bind_group = Self::bind_group(gpu, frame, &uniform, &target, normals, &sampler);
        Self {
            uniform,
            target,
            bind_group,
        }
    }

    fn bind_group(
        gpu: &Gpu,
        frame: &Frame,
        uniform: &wgpu::Buffer,
        target: &ColorTarget,
        normals: &TextureView,
        sampler: &Sampler,
    ) -> wgpu::BindGroup {
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water"),
            layout: &gpu.layouts.water,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(normals),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&target.resolved_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&frame.linear_sampler),
                },
            ],
        })
    }

    /// Bind another normal map (the real one, once it has loaded).
    pub fn rebind(&mut self, gpu: &Gpu, frame: &Frame, normals: &TextureView, sampler: Sampler) {
        self.bind_group =
            Self::bind_group(gpu, frame, &self.uniform, &self.target, normals, &sampler);
    }

    pub fn write(&self, gpu: &Gpu, uniform: &WaterUniform) {
        gpu.queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(uniform));
    }

    /// Bytes the reflection target holds in memory.
    pub fn target_bytes(&self) -> u64 {
        self.target.bytes()
    }
}

impl Drop for WaterGpu {
    fn drop(&mut self) {
        self.uniform.destroy();
    }
}

/// A pass into a scene target, which keeps only its color, resolved when multisampled.
fn scene_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    label: &str,
    target: &ColorTarget,
    clear: wgpu::Color,
) -> wgpu::RenderPass<'a> {
    let resolve_target = target.resolve_target();
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &target.color_view,
            depth_slice: None,
            resolve_target,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear),
                // The multisampled color is spent once resolved; a single-sampled
                // target is the result.
                store: if resolve_target.is_some() {
                    wgpu::StoreOp::Discard
                } else {
                    wgpu::StoreOp::Store
                },
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &target.depth_view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Discard,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// The mesh pages a pass has bound, so consecutive draws from one page skip the
/// rebinding. Each encode starts from none bound: other draws between them (the
/// water, merged shadows) may have changed the bindings.
struct BoundPages {
    vertex: u16,
    index: u16,
}

impl Default for BoundPages {
    fn default() -> Self {
        Self {
            vertex: NO_PAGE,
            index: NO_PAGE,
        }
    }
}

impl BoundPages {
    /// Bind the pages of `range` that are not bound yet, the vertices to slot 0
    /// (merged shadows keep their record bases on slot 1).
    fn bind(&mut self, pass: &mut wgpu::RenderPass, meshes: &super::MeshStore, range: MeshRange) {
        if range.vertex_page != self.vertex {
            let (buffers, page) = meshes.page(range.vertex_page);
            pass.set_vertex_buffer(0, buffers.vertex_buffers(page.family, page.written()));
            self.vertex = range.vertex_page;
        }
        if range.index_page != self.index {
            let (buffers, page) = meshes.page(range.index_page);
            pass.set_index_buffer(
                buffers.index_buffer(page.written()),
                wgpu::IndexFormat::Uint32,
            );
            self.index = range.index_page;
        }
    }
}

struct DrawContext<'a> {
    scene: &'a Scene<'a>,
    frame_groups: &'a FrameGroups,
}

/// Which of a class's pipelines a pass draws it with.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Variant {
    /// Lit color: `main`, after `back` for two-pass transparency.
    Lit,
    /// The sun shadow's `shadow`.
    Shadow,
    /// The occlusion depth pass's `depth`.
    Depth,
}

use Variant::{Depth, Lit, Shadow};

impl DrawContext<'_> {
    /// Encode merged shadow draws. Returns the draw count. The caller binds the
    /// shadow view's frame group.
    fn encode_merged(
        &self,
        pass: &mut wgpu::RenderPass,
        draws: &[MergedDraw],
        bases: &wgpu::Buffer,
        stats: &mut RenderStats,
    ) -> u32 {
        let scene = self.scene;
        pass.set_vertex_buffer(1, bases.slice(..));
        let mut pipeline = usize::MAX;
        let mut pages = BoundPages::default();
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
            if mesh.pipeline != pipeline {
                pass.set_pipeline(&scene.pipelines.fixed().shadow_merged[mesh.pipeline]);
                pipeline = mesh.pipeline;
            }
            if let Some(material) = mesh.material {
                pass.set_bind_group(1, &scene.materials.get(material).binding.bind_group, &[]);
            }
            let range = mesh.range;
            pages.bind(pass, scene.meshes, range);
            pass.draw_indexed(range.indices(), 0, draw.first..draw.first + draw.count);
            count += 1;
            stats.draw_calls += 1;
            stats.triangles += (range.index_count / 3) as u64 * draw.count as u64;
        }
        count
    }

    /// Encode draws with the `variant` pipelines, skipping redundant state changes.
    /// Returns the draw count. The caller binds the view's frame group; pool draws
    /// swap in their own instance buffer and the frame group is restored afterwards.
    fn encode(
        &self,
        pass: &mut wgpu::RenderPass,
        draws: &[Draw],
        view: usize,
        variant: Variant,
        stats: &mut RenderStats,
    ) -> u32 {
        let scene = self.scene;
        let mut last_pipeline = u32::MAX;
        let mut last_material = u32::MAX;
        let mut pages = BoundPages::default();
        let mut bound_pool: Option<u32> = None;
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
            if class.key.pool != bound_pool {
                let group = match class.key.pool {
                    Some(pool) => match scene.pools.at(pool) {
                        Some(entry) => &entry.groups.0[view],
                        None => continue,
                    },
                    None => &self.frame_groups.0[view],
                };
                pass.set_bind_group(0, group, &[]);
                bound_pool = class.key.pool;
            }
            let (pipeline, back, own_material) = match variant {
                Lit => (class.main, class.back, true),
                Shadow => (class.shadow, None, class.shadow_material),
                Depth => (class.depth, None, class.depth_material),
            };
            let Some(pipeline) = pipeline else {
                continue;
            };
            // A depth-only caster that reads no material draws with whichever one is
            // bound; a pass starts with none.
            if class.key.material != last_material && (own_material || last_material == u32::MAX) {
                pass.set_bind_group(
                    1,
                    &scene.materials.get(class.key.material).binding.bind_group,
                    &[],
                );
                last_material = class.key.material;
            }
            pages.bind(pass, scene.meshes, range);
            for pipeline in back.into_iter().chain([pipeline]) {
                if pipeline != last_pipeline {
                    pass.set_pipeline(scene.pipelines.get(pipeline));
                    last_pipeline = pipeline;
                }
                let indices = draw.indices(range.indices());
                stats.triangles += (indices.len() / 3) as u64 * draw.instance_count as u64;
                pass.draw_indexed(
                    indices,
                    0,
                    draw.first_instance..draw.first_instance + draw.instance_count,
                );
                count += 1;
            }
        }
        if bound_pool.is_some() {
            pass.set_bind_group(0, &self.frame_groups.0[view], &[]);
        }
        stats.draw_calls += count;
        count
    }
}
