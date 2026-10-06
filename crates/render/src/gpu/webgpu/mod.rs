//! The WebGPU backend: wgpu on the browser's WebGPU. Its frame encodes the sun
//! shadow, water reflection and main view passes into one command buffer and the
//! output pass into another.

mod context;
mod instance_store;
mod pipelines;
mod precompile;
mod resources;
mod textures;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use wgpu::util::DeviceExt;

pub use context::{Canvas, Gpu};
pub use instance_store::InstanceStore;
pub use pipelines::Pipelines;
pub use resources::{MaterialBinding, PageBuffers};
pub use textures::{Sampler, Texture, TextureView, Uploader};

use super::{FrameUniform, MergedDraw, RenderStats, SAMPLE_COUNT, Scene, WaterUniform, lut};
use crate::draw_list::{Draw, Grouping, MAIN_VIEW, REFLECTION_VIEW, SHADOW_VIEW, VIEW_COUNT};
use crate::mesh_pages::{MeshRange, NO_PAGE};
use context::{ColorTarget, DEPTH_FORMAT};
use resources::uniform_buffer;

/// Opaque draws group by mesh page before material. Every switch is a call Chrome
/// validates in the GPU process, and a page switch costs one or two vertex buffers
/// and often the index buffer where a material is one bind group: grouping by page
/// first cut the GPU process 11-20% against class index order (material first,
/// 6-18%), and neither changed the main thread measurably.
pub const DRAW_GROUPING: Grouping = Grouping::PageFirst;

impl Gpu {
    /// The first GPU validation error or device loss, if any.
    pub fn error(&self) -> Option<String> {
        self.error.get()
    }
}

fn depth_texture(device: &wgpu::Device, label: &str, size: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// A depth texture with its view; dropping it destroys the texture.
struct DepthMap {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl DepthMap {
    fn new(device: &wgpu::Device, label: &str, size: u32) -> Self {
        let texture = depth_texture(device, label, size);
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

const INITIAL_SHADOW_BASES: u32 = 1024;

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
    lut_view: wgpu::TextureView,
    lut_sampler: wgpu::Sampler,
    reflection_sampler: wgpu::Sampler,
    view_uniforms: [wgpu::Buffer; VIEW_COUNT],
    view_groups: FrameGroups,
    instance_records: InstanceStore,
    output_uniform: wgpu::Buffer,
    output_group: wgpu::BindGroup,
    shadow_base_buffer: wgpu::Buffer,
    shadow_base_capacity: u32,
    /// Set once the GPU has run everything submitted before the last
    /// [`Frame::await_gpu`].
    gpu_idle: Arc<AtomicBool>,
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
        let linear_clamp = |label| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            })
        };
        let main_target = ColorTarget::new(device, "main view", width, height, SAMPLE_COUNT);
        let output_uniform = uniform_buffer(device, "output", 16);
        let output_group = output_group(gpu, &main_target, &output_uniform);
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
            lut_view: lut.create_view(&Default::default()),
            lut_sampler: linear_clamp("DFG LUT"),
            reflection_sampler: linear_clamp("water reflection"),
            view_uniforms: [0, 1, 2]
                .map(|_| uniform_buffer(device, "frame", size_of::<FrameUniform>() as u64)),
            // Replaced below, once the frame's own resources exist.
            view_groups: FrameGroups(std::array::from_fn(|_| output_group.clone())),
            instance_records,
            output_uniform,
            output_group,
            shadow_base_buffer: base_buffer(device, INITIAL_SHADOW_BASES),
            shadow_base_capacity: INITIAL_SHADOW_BASES,
            gpu_idle: Arc::new(AtomicBool::new(true)),
        };
        frame.view_groups = frame.frame_groups(gpu, &frame.instance_records);
        frame
    }

    pub fn size(&self) -> (u32, u32) {
        (self.canvas.config.width, self.canvas.config.height)
    }

    /// Resize the canvas drawing buffer and the main view's target.
    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        let limit = gpu.device.limits().max_texture_dimension_2d;
        let width = width.clamp(1, limit);
        let height = height.clamp(1, limit);
        let config = &mut self.canvas.config;
        if config.width == width && config.height == height {
            return;
        }
        config.width = width;
        config.height = height;
        self.canvas.surface.configure(&gpu.device, config);
        self.main_target = ColorTarget::new(&gpu.device, "main view", width, height, SAMPLE_COUNT);
        self.output_group = output_group(gpu, &self.main_target, &self.output_uniform);
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

    pub fn has_static_shadow(&self) -> bool {
        self.static_shadow_map.is_some()
    }

    /// The view's instance records.
    pub fn instances(&self) -> &InstanceStore {
        &self.instance_records
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
                        resource: wgpu::BindingResource::Sampler(&self.lut_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: instances.binding(),
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

    pub fn write_output(&self, gpu: &Gpu, exposure: f32) {
        // The canvas's rows run top-down like the HDR target's: row = 0 + 1 × y.
        gpu.queue.write_buffer(
            &self.output_uniform,
            0,
            bytemuck::bytes_of(&[exposure, 0.0, 1.0, 0.0]),
        );
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

    /// WebGPU runs wgpu's callbacks from the browser's event loop.
    pub fn gpu_idle(&self, _gpu: &Gpu) -> bool {
        self.gpu_idle.load(Ordering::Acquire)
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
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("scene"),
        });
        self.encode_scene(&mut encoder, scene, stats);
        gpu.queue.submit([encoder.finish()]);
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
        Ok(())
    }

    /// Draw every pass once, the output into an offscreen probe, so the browser
    /// finishes compiling before gameplay needs the pipelines.
    pub fn warm_up(
        &mut self,
        gpu: &Gpu,
        scene: &Scene,
        stats: &mut RenderStats,
    ) -> Result<(), String> {
        let device = &gpu.device;
        let probe = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("warm-up output"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.canvas.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let probe_view = probe.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("warm-up"),
        });
        self.encode_scene(&mut encoder, scene, stats);
        self.encode_output(&mut encoder, &probe_view, scene.pipelines, stats);
        gpu.queue.submit([encoder.finish()]);
        probe.destroy();
        Ok(())
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
            static_count += draws.encode(&mut pass, scene.static_shadow_draws, SHADOW_VIEW, stats);
            static_count += draws.encode_merged(
                &mut pass,
                scene.static_merged_draws,
                &self.shadow_base_buffer,
                stats,
            );
        }
        if scene.copy_static {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &static_shadow().texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::DepthOnly,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.shadow_map.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::DepthOnly,
                },
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
        stats.reflection_draw_calls = 0;
        stats.reflection_triangles = 0;
        if let (true, Some(water)) = (scene.reflection, scene.water) {
            let mut pass = scene_pass(encoder, "water reflection", &water.gpu.target, clear);
            pass.set_bind_group(0, &self.view_groups.0[REFLECTION_VIEW], &[]);
            let view = &scene.views[REFLECTION_VIEW];
            let count = draws.encode(&mut pass, &view.opaque, REFLECTION_VIEW, stats)
                + draws.encode(&mut pass, &view.transparent, REFLECTION_VIEW, stats);
            stats.reflection_draw_calls = count;
            stats.reflection_triangles = stats.triangles - stats.shadow_triangles;
        }
        {
            let mut pass = scene_pass(encoder, "main view", &self.main_target, clear);
            pass.set_bind_group(0, &self.view_groups.0[MAIN_VIEW], &[]);
            let view = &scene.views[MAIN_VIEW];
            draws.encode(&mut pass, &view.opaque, MAIN_VIEW, stats);
            if let Some(water) = scene.water
                && let range = scene.meshes.get(water.mesh).range
                && !range.is_empty()
            {
                pass.set_pipeline(&scene.pipelines.fixed().water);
                pass.set_bind_group(1, &water.gpu.bind_group, &[]);
                let (buffers, page) = scene.meshes.page(range.vertex_page);
                pass.set_vertex_buffer(0, buffers.vertex_buffers(page.family, page.written()).0);
                let (buffers, page) = scene.meshes.page(range.index_page);
                pass.set_index_buffer(
                    buffers.index_buffer(page.written()),
                    wgpu::IndexFormat::Uint32,
                );
                pass.draw_indexed(range.indices(), 0, 0..1);
                stats.draw_calls += 1;
                stats.triangles += range.index_count as u64 / 3;
            }
            draws.encode(&mut pass, &view.transparent, MAIN_VIEW, stats);
        }
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
        pass.set_bind_group(0, &self.output_group, &[]);
        pass.draw(0..3, 0..1);
        stats.draw_calls += 1;
    }
}

fn output_group(gpu: &Gpu, target: &ColorTarget, uniform: &wgpu::Buffer) -> wgpu::BindGroup {
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("output"),
        layout: &gpu.layouts.output,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&target.resolved_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform.as_entire_binding(),
            },
        ],
    })
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
        let target = ColorTarget::new(&gpu.device, "water reflection", size, size, SAMPLE_COUNT);
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
                    resource: wgpu::BindingResource::Sampler(&frame.reflection_sampler),
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
}

impl Drop for WaterGpu {
    fn drop(&mut self) {
        self.uniform.destroy();
    }
}

fn scene_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    label: &str,
    target: &ColorTarget,
    clear: wgpu::Color,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &target.color_view,
            depth_slice: None,
            resolve_target: Some(&target.resolved_view),
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear),
                store: wgpu::StoreOp::Discard,
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
/// water, merged shadows on slot 1) may have changed the bindings.
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
    /// Bind the pages of `range` that are not bound yet. A surface page's effect
    /// vec4s go to slot 1; merged shadows keep their record bases there instead.
    fn bind(&mut self, pass: &mut wgpu::RenderPass, meshes: &super::MeshStore, range: MeshRange) {
        if range.vertex_page != self.vertex {
            let (buffers, page) = meshes.page(range.vertex_page);
            let (vertices, extra) = buffers.vertex_buffers(page.family, page.written());
            pass.set_vertex_buffer(0, vertices);
            if let Some(extra) = extra {
                pass.set_vertex_buffer(1, extra);
            }
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

impl DrawContext<'_> {
    fn encode_merged(
        &self,
        pass: &mut wgpu::RenderPass,
        draws: &[MergedDraw],
        bases: &wgpu::Buffer,
        stats: &mut RenderStats,
    ) -> u32 {
        let scene = self.scene;
        pass.set_bind_group(0, &self.frame_groups.0[SHADOW_VIEW], &[]);
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

    /// Encode draws, skipping redundant state changes. Returns the draw count.
    /// The caller binds the view's frame group; pool draws swap in their own
    /// instance buffer and the frame group is restored afterwards.
    fn encode(
        &self,
        pass: &mut wgpu::RenderPass,
        draws: &[Draw],
        view: usize,
        stats: &mut RenderStats,
    ) -> u32 {
        let scene = self.scene;
        let shadow = view == SHADOW_VIEW;
        let mut last_pipeline = u32::MAX;
        let mut last_material = u32::MAX;
        let mut pages = BoundPages::default();
        let mut bound_pool: Option<u32> = None;
        let mut count = 0;
        for draw in draws {
            let Some(class) = &scene.classes[draw.class as usize] else {
                continue;
            };
            // An empty mesh has no page to bind and nothing to draw.
            let range = scene.meshes.get(class.key.mesh).range;
            if range.is_empty() {
                continue;
            }
            if class.pool != bound_pool {
                let group = match class.pool {
                    Some(pool) => match scene.pools.at(pool) {
                        Some(entry) => &entry.groups.0[view],
                        None => continue,
                    },
                    None => &self.frame_groups.0[view],
                };
                pass.set_bind_group(0, group, &[]);
                bound_pool = class.pool;
            }
            let Some(pipeline) = (if shadow { class.shadow } else { class.main }) else {
                continue;
            };
            let back = if shadow { None } else { class.back };
            if class.key.material != last_material {
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
                pass.draw_indexed(
                    range.indices(),
                    0,
                    draw.first_instance..draw.first_instance + draw.instance_count,
                );
                count += 1;
                stats.triangles += (range.index_count / 3) as u64 * draw.instance_count as u64;
            }
        }
        if bound_pool.is_some() {
            pass.set_bind_group(0, &self.frame_groups.0[view], &[]);
        }
        stats.draw_calls += count;
        count
    }
}
