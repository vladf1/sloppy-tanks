use crate::physics::{INITIAL_BLOCKS, MAX_SHOTS, STATIC_BLOCKS, Simulation};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;

const SAMPLE_COUNT: u32 = 4;
const SHADOW_SIZE: u32 = 1024;
const INSTANCE_CAPACITY: usize = INITIAL_BLOCKS + MAX_SHOTS + STATIC_BLOCKS;
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    model: [f32; 16],
    color: [f32; 4],
}

#[wasm_bindgen]
pub struct Lab {
    simulation: Simulation,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    frame_buffer: wgpu::Buffer,
    frame_group: wgpu::BindGroup,
    shadow_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instances: Vec<Instance>,
    depth: wgpu::Texture,
    msaa: wgpu::Texture,
    shadow: wgpu::Texture,
    error: Arc<Mutex<Option<String>>>,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

fn texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    samples: u32,
    shadow: bool,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(if shadow {
            "shadow map"
        } else {
            "frame attachment"
        }),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | if shadow {
                wgpu::TextureUsages::TEXTURE_BINDING
            } else {
                wgpu::TextureUsages::empty()
            },
        view_formats: &[],
    })
}

#[wasm_bindgen]
impl Lab {
    pub async fn create(canvas: web_sys::HtmlCanvasElement) -> Result<Lab, JsValue> {
        console_error_panic_hook::set_once();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|e| JsValue::from_str(&format!("WebGPU adapter unavailable: {e}")))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Rust physics lab"),
                ..Default::default()
            })
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let error = Arc::new(Mutex::new(None));
        let lost_error = error.clone();
        device.set_device_lost_callback(move |reason, message| {
            if reason != wgpu::DeviceLostReason::Destroyed {
                *lost_error.lock().unwrap() =
                    Some(format!("GPU device lost: {message}. Reload to restart."));
            }
        });
        let validation_error = error.clone();
        device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
            *validation_error.lock().unwrap() = Some(format!("WebGPU error: {e}"));
        }));
        let mut config = surface
            .get_default_config(&adapter, 1, 1)
            .ok_or_else(|| JsValue::from_str("No supported canvas configuration"))?;
        let output_format = config.format.add_srgb_suffix();
        config.view_formats = vec![output_format];
        surface.configure(&device, &config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lit blocks + grid + shadows"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera and light"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow sampling"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&frame_layout), Some(&shadow_layout)],
            immediate_size: 0,
        });
        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&frame_layout)],
                immediate_size: 0,
            });
        let targets = [Some(wgpu::ColorTargetState {
            format: output_format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let make_pipeline = |shadow: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(if shadow { "shadow" } else { "lit scene" }),
                layout: Some(if shadow {
                    &shadow_pipeline_layout
                } else {
                    &pipeline_layout
                }),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(if shadow { "vs_shadow" } else { "vs_main" }),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &INSTANCE_ATTRIBUTES,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: if shadow { 1 } else { SAMPLE_COUNT },
                    ..Default::default()
                },
                fragment: if shadow {
                    None
                } else {
                    Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        targets: &targets,
                    })
                },
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = make_pipeline(false);
        let shadow_pipeline = make_pipeline(true);
        let frame_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("frame uniforms"),
            contents: bytemuck::cast_slice(&[0.0f32; 32]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let frame_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });
        let shadow = texture(
            &device,
            SHADOW_SIZE,
            SHADOW_SIZE,
            wgpu::TextureFormat::Depth32Float,
            1,
            true,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shadow_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &shadow_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &shadow.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bounded block instances"),
            size: (INSTANCE_CAPACITY * size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth = texture(
            &device,
            1,
            1,
            wgpu::TextureFormat::Depth32Float,
            SAMPLE_COUNT,
            false,
        );
        let msaa = texture(&device, 1, 1, output_format, SAMPLE_COUNT, false);
        Ok(Lab {
            simulation: Simulation::new(),
            surface,
            device,
            queue,
            config,
            pipeline,
            shadow_pipeline,
            frame_buffer,
            frame_group,
            shadow_group,
            instance_buffer,
            instances: Vec::with_capacity(INSTANCE_CAPACITY),
            depth,
            msaa,
            shadow,
            error,
            yaw: 0.60,
            pitch: 0.64,
            distance: 25.0,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let limit = self.device.limits().max_texture_dimension_2d;
        let width = width.clamp(1, limit);
        let height = height.clamp(1, limit);
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth.destroy();
        self.msaa.destroy();
        self.depth = texture(
            &self.device,
            width,
            height,
            wgpu::TextureFormat::Depth32Float,
            SAMPLE_COUNT,
            false,
        );
        self.msaa = texture(
            &self.device,
            width,
            height,
            self.config.format.add_srgb_suffix(),
            SAMPLE_COUNT,
            false,
        );
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.006;
        self.pitch = (self.pitch + dy * 0.005).clamp(0.18, 1.35);
    }
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance * (delta * 0.001).exp()).clamp(12.0, 36.0);
    }
    pub fn launch(&mut self) {
        self.simulation.launch();
    }
    pub fn reset(&mut self) {
        self.simulation = Simulation::new();
    }
    pub fn body_count(&self) -> usize {
        self.simulation.dynamic_count()
    }
    pub fn active_count(&self) -> usize {
        self.simulation.active_count()
    }
    pub fn ticks(&self) -> u32 {
        self.simulation.ticks
    }
    pub fn stack_displacement(&self) -> f32 {
        self.simulation.stack_displacement()
    }

    pub fn frame(&mut self, seconds: f64, paused: bool) -> Result<(), JsValue> {
        if let Some(error) = self.error.lock().unwrap().as_ref() {
            return Err(JsValue::from_str(error));
        }
        if !paused {
            self.simulation.advance(seconds);
        }
        let target = Vec3::new(0.0, 0.8, 0.0);
        let eye = target
            + Vec3::new(
                self.yaw.sin() * self.pitch.cos(),
                self.pitch.sin(),
                self.yaw.cos() * self.pitch.cos(),
            ) * self.distance;
        let camera = glam::camera::rh::proj::directx::perspective(
            45.0f32.to_radians(),
            self.config.width as f32 / self.config.height as f32,
            0.1,
            100.0,
        ) * glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y);
        let light =
            glam::camera::rh::proj::directx::orthographic(-11.0, 11.0, -11.0, 11.0, 0.1, 45.0)
                * glam::camera::rh::view::look_at_mat4(
                    Vec3::new(-8.0, 16.0, 10.0),
                    Vec3::ZERO,
                    Vec3::Y,
                );
        self.queue.write_buffer(
            &self.frame_buffer,
            0,
            bytemuck::cast_slice(&[camera.to_cols_array(), light.to_cols_array()]),
        );
        self.instances.clear();
        for block in &self.simulation.blocks {
            self.instances.push(Instance {
                model: self.simulation.transform(block).to_cols_array(),
                color: block.color,
            });
        }
        self.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&self.instances),
        );
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            error => {
                return Err(JsValue::from_str(&format!(
                    "Canvas unavailable: {error:?}. Reload to restart."
                )));
            }
        };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format.add_srgb_suffix()),
            ..Default::default()
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let shadow_view = self.shadow.create_view(&Default::default());
        let depth_view = self.depth.create_view(&Default::default());
        let msaa_view = self.msaa.create_view(&Default::default());
        for shadow in [true, false] {
            let colors = [Some(wgpu::RenderPassColorAttachment {
                view: &msaa_view,
                depth_slice: None,
                resolve_target: Some(&view),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.025,
                        g: 0.041,
                        b: 0.054,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Discard,
                },
            })];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(if shadow { "shadow" } else { "scene" }),
                color_attachments: if shadow { &[] } else { &colors },
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: if shadow { &shadow_view } else { &depth_view },
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: if shadow {
                            wgpu::StoreOp::Store
                        } else {
                            wgpu::StoreOp::Discard
                        },
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(if shadow {
                &self.shadow_pipeline
            } else {
                &self.pipeline
            });
            pass.set_bind_group(0, &self.frame_group, &[]);
            if !shadow {
                pass.set_bind_group(1, &self.shadow_group, &[]);
            }
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            pass.draw(0..36, 0..self.instances.len() as u32);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(output);
        Ok(())
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        self.device.destroy();
    }
}
