//! WebGPU resources and command submission. Scene preparation and draw lists live in the parent.
pub(super) use super::context::{ColorTarget, Context, DEPTH_FORMAT};
use super::*;
use wgpu::util::DeviceExt;
pub(super) use wgpu::{Buffer, Device, Queue};
pub(super) type FrameGroup = wgpu::BindGroup;
pub(super) type MaterialGroup = wgpu::BindGroup;
pub(super) type WaterGroup = wgpu::BindGroup;
pub(super) fn write_buffer(queue: &Queue, buffer: &Buffer, offset: u64, data: &[u8]) {
    queue.write_buffer(buffer, offset, data);
}
pub(super) fn mesh_buffer(device: &Device, label: &str, size: u64, index: bool) -> Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: (if index {
            wgpu::BufferUsages::INDEX
        } else {
            wgpu::BufferUsages::VERTEX
        }) | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
pub(super) struct FrameResources {
    pub(super) ctx: Context,
    pub(super) layouts: Layouts,
    pub(super) main_target: ColorTarget,
    pub(super) shadow_map: wgpu::Texture,
    pub(super) shadow_view: wgpu::TextureView,
    pub(super) static_shadow_map: wgpu::Texture,
    pub(super) static_shadow_view: wgpu::TextureView,
    pub(super) dummy_depth: wgpu::TextureView,
    pub(super) shadow_sampler: wgpu::Sampler,
    pub(super) lut_view: wgpu::TextureView,
    pub(super) lut_sampler: wgpu::Sampler,
    pub(super) reflection_sampler: wgpu::Sampler,
    pub(super) view_uniforms: [wgpu::Buffer; VIEW_COUNT],
    pub(super) view_groups: Vec<wgpu::BindGroup>,
    pub(super) instance_records: InstanceStore,
    pub(super) output_uniform: wgpu::Buffer,
    pub(super) output_group: wgpu::BindGroup,
    pub(super) shadow_base_buffer: wgpu::Buffer,
    pub(super) shadow_base_capacity: u32,
    pub(super) gpu_idle: Arc<AtomicBool>,
}
impl FrameResources {
    pub(super) fn size(&self) -> (u32, u32) {
        (self.ctx.config.width, self.ctx.config.height)
    }
    pub(super) fn error(&self) -> Option<String> {
        self.ctx.error.get()
    }
    pub(super) async fn new(canvas: web_sys::HtmlCanvasElement) -> Result<Self, String> {
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let ctx = Context::new(canvas).await?;
        let device = &ctx.device;
        let layouts = Layouts::new(device);
        let sun_shadow = SunShadow::default();
        let shadow_map = depth_texture(device, "sun shadow map", sun_shadow.map_size);
        let static_shadow_map = depth_texture(device, "fixed scenery shadow", sun_shadow.map_size);
        let dummy_depth =
            depth_texture(device, "shadow pass placeholder", 1).create_view(&Default::default());
        let lut = device.create_texture_with_data(
            &ctx.queue,
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
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow compare"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let view_uniforms =
            [0, 1, 2].map(|_| uniform_buffer(device, "frame", size_of::<FrameUniform>() as u64));
        let instance_records = InstanceStore::new(device, "instances", INITIAL_INSTANCE_CAPACITY);
        let main_target = ColorTarget::new(device, "main view", width, height, SAMPLE_COUNT);
        let output_uniform = uniform_buffer(device, "output", 16);
        let output_group = Renderer::output_group(device, &layouts, &main_target, &output_uniform);
        Ok(Self {
            layouts,
            shadow_view: shadow_map.create_view(&Default::default()),
            shadow_map,
            static_shadow_view: static_shadow_map.create_view(&Default::default()),
            static_shadow_map,
            dummy_depth,
            shadow_sampler,
            lut_view: lut.create_view(&Default::default()),
            lut_sampler: linear_clamp("DFG LUT"),
            reflection_sampler: linear_clamp("water reflection"),
            view_uniforms,
            view_groups: Vec::new(),
            instance_records,
            output_uniform,
            output_group,
            main_target,
            shadow_base_buffer: base_buffer(device, INITIAL_SHADOW_BASES),
            shadow_base_capacity: INITIAL_SHADOW_BASES,
            gpu_idle: Arc::new(AtomicBool::new(true)),
            ctx,
        })
    }
}
pub(super) fn uniform_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
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

pub(super) fn base_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shadow bases"),
        size: capacity as u64 * 4,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl Renderer {
    fn output_group(
        device: &wgpu::Device,
        layouts: &Layouts,
        target: &ColorTarget,
        uniform: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("output"),
            layout: &layouts.output,
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
    pub(super) fn frame_group(&self, view: usize, instances: &InstanceStore) -> wgpu::BindGroup {
        let shadow = if view == SHADOW_VIEW {
            &self.gpu.dummy_depth
        } else {
            &self.gpu.shadow_view
        };
        self.gpu
            .ctx
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("frame"),
                layout: &self.gpu.layouts.frame,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.gpu.view_uniforms[view].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(shadow),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.gpu.shadow_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&self.gpu.lut_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&self.gpu.lut_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: instances.binding(),
                    },
                ],
            })
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        let limit = self.gpu.ctx.device.limits().max_texture_dimension_2d;
        let width = width.clamp(1, limit);
        let height = height.clamp(1, limit);
        if self.gpu.ctx.config.width == width && self.gpu.ctx.config.height == height {
            return;
        }
        self.gpu.ctx.config.width = width;
        self.gpu.ctx.config.height = height;
        self.gpu
            .ctx
            .surface
            .configure(&self.gpu.ctx.device, &self.gpu.ctx.config);
        self.gpu.main_target.destroy();
        self.gpu.main_target = ColorTarget::new(
            &self.gpu.ctx.device,
            "main view",
            width,
            height,
            SAMPLE_COUNT,
        );
        self.gpu.output_group = Self::output_group(
            &self.gpu.ctx.device,
            &self.gpu.layouts,
            &self.gpu.main_target,
            &self.gpu.output_uniform,
        );
    }
    pub(super) fn water_group(
        &mut self,
        normals: &TextureRef,
        uniform: &wgpu::Buffer,
        target: &ColorTarget,
    ) -> (wgpu::BindGroup, bool) {
        let device = &self.gpu.ctx.device;
        let sampler = self.textures.sampler(device, Some(normals));
        let (view, ready) = self.textures.view(normals);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water"),
            layout: &self.gpu.layouts.water,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&target.resolved_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.gpu.reflection_sampler),
                },
            ],
        });
        (bind_group, !ready)
    }
    pub fn await_gpu(&mut self) {
        let idle = Arc::new(AtomicBool::new(false));
        self.gpu.gpu_idle = idle.clone();
        self.gpu
            .ctx
            .queue
            .on_submitted_work_done(move || idle.store(true, Ordering::Release));
    }
    pub fn gpu_idle(&self) -> bool {
        self.gpu.gpu_idle.load(Ordering::Acquire)
    }
    fn encode_scene(&mut self, encoder: &mut wgpu::CommandEncoder, reflection: bool) {
        let stats = &mut self.stats;
        stats.draw_calls = 0;
        stats.triangles = 0;
        let background = hex_to_linear(self.environment.background);
        let clear = wgpu::Color {
            r: background[0] as f64,
            g: background[1] as f64,
            b: background[2] as f64,
            a: 1.0,
        };
        let draws = DrawContext {
            classes: &self.classes,
            meshes: &self.meshes,
            materials: &self.materials,
            pipelines: &self.pipelines,
            pools: &self.pools,
            models: &self.models,
            frame_groups: &self.gpu.view_groups,
        };
        let rebuild_static =
            self.cache_static_shadow && self.static_shadow_dirty && self.sun_shadow.enabled;
        let mut static_count = 0;
        if rebuild_static {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fixed scenery shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.gpu.static_shadow_view,
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
            pass.set_bind_group(0, &self.gpu.view_groups[SHADOW_VIEW], &[]);
            static_count += draws.encode(&mut pass, &self.static_shadow_draws, SHADOW_VIEW, stats);
            static_count += draws.encode_merged(
                &mut pass,
                &self.static_merged_draws,
                &self.gpu.shadow_base_buffer,
                stats,
            );
        }
        let copy_static = self.sun_shadow.enabled && self.cache_static_shadow;
        if copy_static {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.gpu.static_shadow_map,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::DepthOnly,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.gpu.shadow_map,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::DepthOnly,
                },
                self.gpu.shadow_map.size(),
            );
        }
        if copy_static {
            self.static_shadow_dirty = false;
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.gpu.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: if copy_static {
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

            pass.set_bind_group(0, &self.gpu.view_groups[SHADOW_VIEW], &[]);
            let mut count = draws.encode(
                &mut pass,
                &self.views[SHADOW_VIEW].opaque,
                SHADOW_VIEW,
                stats,
            );
            count += draws.encode_merged(
                &mut pass,
                &self.merged_draws,
                &self.gpu.shadow_base_buffer,
                stats,
            );
            stats.shadow_draw_calls = count + static_count;
            stats.shadow_triangles = stats.triangles;
        }
        stats.reflection_draw_calls = 0;
        stats.reflection_triangles = 0;
        if let (true, Some(water)) = (reflection, &self.water) {
            let mut pass = scene_pass(encoder, "water reflection", &water.target, clear);
            pass.set_bind_group(0, &self.gpu.view_groups[REFLECTION_VIEW], &[]);
            let view = &self.views[REFLECTION_VIEW];
            let count = draws.encode(&mut pass, &view.opaque, REFLECTION_VIEW, stats)
                + draws.encode(&mut pass, &view.transparent, REFLECTION_VIEW, stats);
            stats.reflection_draw_calls = count;
            stats.reflection_triangles = stats.triangles - stats.shadow_triangles;
        }
        {
            let mut pass = scene_pass(encoder, "main view", &self.gpu.main_target, clear);
            pass.set_bind_group(0, &self.gpu.view_groups[MAIN_VIEW], &[]);
            let view = &self.views[MAIN_VIEW];
            draws.encode(&mut pass, &view.opaque, MAIN_VIEW, stats);
            if let Some(water) = &self.water
                && let range = self.meshes.get(water.mesh).range
                && !range.is_empty()
            {
                pass.set_pipeline(&self.pipelines.fixed().water);
                pass.set_bind_group(1, &water.bind_group, &[]);
                pass.set_vertex_buffer(0, self.meshes.vertex_buffers(range.vertex_page).0);
                pass.set_index_buffer(
                    self.meshes.index_buffer(range.index_page),
                    wgpu::IndexFormat::Uint32,
                );
                pass.draw_indexed(range.indices(), 0, 0..1);
                stats.draw_calls += 1;
                stats.triangles += range.index_count as u64 / 3;
            }
            draws.encode(&mut pass, &view.transparent, MAIN_VIEW, stats);
            stats.main_triangles =
                stats.triangles - stats.shadow_triangles - stats.reflection_triangles;
        }
    }
    fn encode_output(&mut self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        {
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
            pass.set_pipeline(&self.pipelines.fixed().output);
            pass.set_bind_group(0, &self.gpu.output_group, &[]);
            pass.draw(0..3, 0..1);
            self.stats.draw_calls += 1;
        }
    }
    pub(super) fn resize_shadow(&mut self, size: u32) {
        self.gpu.shadow_map.destroy();
        self.gpu.static_shadow_map.destroy();
        self.gpu.static_shadow_map =
            depth_texture(&self.gpu.ctx.device, "fixed scenery shadow", size.max(1));
        self.gpu.static_shadow_view = self.gpu.static_shadow_map.create_view(&Default::default());

        self.gpu.shadow_map = depth_texture(&self.gpu.ctx.device, "sun shadow map", size.max(1));
        self.gpu.shadow_view = self.gpu.shadow_map.create_view(&Default::default());
        self.rebuild_view_groups();
    }
    pub(super) fn warm_output(&mut self) {
        let device = &self.gpu.ctx.device;
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
            format: self.gpu.ctx.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let probe_view = probe.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("warm-up"),
        });
        self.encode_scene(&mut encoder, false);
        self.encode_output(&mut encoder, &probe_view);
        self.gpu.ctx.queue.submit([encoder.finish()]);
        probe.destroy();
    }
    pub(super) fn draw_output(&mut self) -> Result<(), String> {
        // The scene and the canvas go in separate command buffers. WebKit paces a
        // WebGPU canvas by the GPU time of the command buffers that write its texture
        // (`WebGPUFramePacer`) and lowers the frame rate when that exceeds a display
        // frame; with the whole frame in one buffer the scene counted against the
        // canvas and Safari settled at 30 fps. The output buffer is one full-screen
        // triangle.
        let mut encoder =
            self.gpu
                .ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("scene"),
                });
        self.encode_scene(&mut encoder, self.reflection_active);
        self.gpu.ctx.queue.submit([encoder.finish()]);
        let output = match self.gpu.ctx.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.gpu
                    .ctx
                    .surface
                    .configure(&self.gpu.ctx.device, &self.gpu.ctx.config);
                return Ok(());
            }
            error => return Err(format!("Canvas unavailable: {error:?}. Reload to restart.")),
        };
        let view = output.texture.create_view(&Default::default());
        let mut encoder =
            self.gpu
                .ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("output"),
                });
        self.encode_output(&mut encoder, &view);
        self.gpu.ctx.queue.submit([encoder.finish()]);
        self.gpu.ctx.queue.present(output);
        Ok(())
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
    fn bind(&mut self, pass: &mut wgpu::RenderPass, meshes: &MeshStore, range: MeshRange) {
        if range.vertex_page != self.vertex {
            let (vertices, extra) = meshes.vertex_buffers(range.vertex_page);
            pass.set_vertex_buffer(0, vertices);
            if let Some(extra) = extra {
                pass.set_vertex_buffer(1, extra);
            }
            self.vertex = range.vertex_page;
        }
        if range.index_page != self.index {
            pass.set_index_buffer(
                meshes.index_buffer(range.index_page),
                wgpu::IndexFormat::Uint32,
            );
            self.index = range.index_page;
        }
    }
}

struct DrawContext<'a> {
    classes: &'a [Option<ClassEntry>],
    meshes: &'a MeshStore,
    materials: &'a MaterialStore,
    pipelines: &'a Pipelines,
    pools: &'a Slab<PoolEntry>,
    models: &'a Slab<ModelEntry>,
    frame_groups: &'a [wgpu::BindGroup],
}

impl DrawContext<'_> {
    fn encode_merged(
        &self,
        pass: &mut wgpu::RenderPass,
        draws: &[MergedDraw],
        bases: &wgpu::Buffer,
        stats: &mut RenderStats,
    ) -> u32 {
        pass.set_bind_group(0, &self.frame_groups[SHADOW_VIEW], &[]);
        pass.set_vertex_buffer(1, bases.slice(..));
        let mut pipeline = usize::MAX;
        let mut pages = BoundPages::default();
        let mut count = 0;
        for draw in draws {
            let Some(mesh) = self
                .models
                .at(draw.model)
                .and_then(|model| model.shadow.get(draw.group as usize))
                .filter(|mesh| !mesh.range.is_empty())
            else {
                continue;
            };
            if mesh.pipeline != pipeline {
                pass.set_pipeline(&self.pipelines.fixed().shadow_merged[mesh.pipeline]);
                pipeline = mesh.pipeline;
            }
            if let Some(material) = mesh.material {
                pass.set_bind_group(1, &self.materials.get(material).bind_group, &[]);
            }
            let range = mesh.range;
            pages.bind(pass, self.meshes, range);
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
        let shadow = view == SHADOW_VIEW;
        let mut last_pipeline = u32::MAX;
        let mut last_material = u32::MAX;
        let mut pages = BoundPages::default();
        let mut bound_pool: Option<u32> = None;
        let mut count = 0;
        for draw in draws {
            let Some(class) = &self.classes[draw.class as usize] else {
                continue;
            };
            // An empty mesh has no page to bind and nothing to draw.
            let range = self.meshes.get(class.key.mesh).range;
            if range.is_empty() {
                continue;
            }
            if class.pool != bound_pool {
                let group = match class.pool {
                    Some(pool) => match self.pools.at(pool) {
                        Some(entry) => &entry.groups[view],
                        None => continue,
                    },
                    None => &self.frame_groups[view],
                };
                pass.set_bind_group(0, group, &[]);
                bound_pool = class.pool;
            }
            let Some(pipeline) = (if shadow { class.shadow } else { class.main }) else {
                continue;
            };
            let back = if shadow { None } else { class.back };
            if class.key.material != last_material {
                pass.set_bind_group(1, &self.materials.get(class.key.material).bind_group, &[]);
                last_material = class.key.material;
            }
            pages.bind(pass, self.meshes, range);
            for pipeline in back.into_iter().chain([pipeline]) {
                if pipeline != last_pipeline {
                    pass.set_pipeline(self.pipelines.get(pipeline));
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
            pass.set_bind_group(0, &self.frame_groups[view], &[]);
        }
        stats.draw_calls += count;
        count
    }
}
