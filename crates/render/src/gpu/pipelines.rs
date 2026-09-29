//! Render pipelines, cached by `PipelineKey` for the page's lifetime (the game's
//! variants are bounded, and recompiling them after every round reset would stall;
//! see `keepReleasedPipelines` in the former renderer.ts). Shader modules are
//! cached by `ShaderKey`.

use std::collections::HashMap;

use sloppy_core::scene::Side;

use crate::effects::EffectRegistry;
use crate::gpu::context::{DEPTH_FORMAT, HDR_FORMAT};
use crate::gpu::resources::Layouts;
use crate::model::Vertex;
use crate::shader::{BlendMode, Pass, PipelineKey, ShaderKey, shader_source, water_source};

pub const SAMPLE_COUNT: u32 = 4;

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    0 => Float32x3,
    1 => Float32x3,
    2 => Float32x2,
    3 => Float32x4,
];
const EXTRA_ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
    4 => Float32x4,
    5 => Float32x4,
];

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &VERTEX_ATTRIBUTES,
    }
}

fn cull_mode(side: Side) -> Option<wgpu::Face> {
    match side {
        Side::Front => Some(wgpu::Face::Back),
        Side::Back => Some(wgpu::Face::Front),
        Side::Double => None,
    }
}

fn blend_state(mode: BlendMode) -> Option<wgpu::BlendState> {
    use wgpu::{BlendComponent, BlendFactor, BlendOperation};
    let component = |src_factor, dst_factor| BlendComponent {
        src_factor,
        dst_factor,
        operation: BlendOperation::Add,
    };
    match mode {
        BlendMode::Replace => None,
        BlendMode::Normal => Some(wgpu::BlendState {
            color: component(BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha),
            alpha: component(BlendFactor::One, BlendFactor::OneMinusSrcAlpha),
        }),
        BlendMode::Additive => Some(wgpu::BlendState {
            color: component(BlendFactor::SrcAlpha, BlendFactor::One),
            alpha: component(BlendFactor::One, BlendFactor::One),
        }),
    }
}

pub struct Pipelines {
    modules: HashMap<ShaderKey, wgpu::ShaderModule>,
    pipelines: Vec<wgpu::RenderPipeline>,
    index: HashMap<PipelineKey, u32>,
    surface_layout: wgpu::PipelineLayout,
    pub water: wgpu::RenderPipeline,
    pub output: wgpu::RenderPipeline,
    /// Pipelines created since the counter was last read (warm-up progress).
    pub created: u32,
}

impl Pipelines {
    pub fn new(
        device: &wgpu::Device,
        layouts: &Layouts,
        canvas_format: wgpu::TextureFormat,
    ) -> Self {
        let surface_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("surface"),
            bind_group_layouts: &[Some(&layouts.frame), Some(&layouts.material)],
            immediate_size: 0,
        });
        let water_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water"),
            bind_group_layouts: &[Some(&layouts.frame), Some(&layouts.water)],
            immediate_size: 0,
        });
        let water_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("water"),
            source: wgpu::ShaderSource::Wgsl(water_source().into()),
        });
        let water = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water"),
            layout: Some(&water_layout),
            vertex: wgpu::VertexState {
                module: &water_module,
                entry_point: Some("vs_water"),
                compilation_options: Default::default(),
                buffers: &[Some(vertex_layout())],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLE_COUNT,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &water_module,
                entry_point: Some("fs_water"),
                compilation_options: Default::default(),
                targets: &[Some(HDR_FORMAT.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let output_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("output"),
            bind_group_layouts: &[Some(&layouts.output)],
            immediate_size: 0,
        });
        let output_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("output"),
            source: wgpu::ShaderSource::Wgsl(crate::shader::OUTPUT_WGSL.into()),
        });
        let output = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("output"),
            layout: Some(&output_layout),
            vertex: wgpu::VertexState {
                module: &output_module,
                entry_point: Some("vs_fullscreen"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &output_module,
                entry_point: Some("fs_output"),
                compilation_options: Default::default(),
                targets: &[Some(canvas_format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            modules: HashMap::new(),
            pipelines: Vec::new(),
            index: HashMap::new(),
            surface_layout,
            water,
            output,
            created: 3,
        }
    }

    pub fn find(&self, key: &PipelineKey) -> Option<u32> {
        self.index.get(key).copied()
    }

    pub fn get(&self, index: u32) -> &wgpu::RenderPipeline {
        &self.pipelines[index as usize]
    }

    /// The pipeline for a key, compiling it on first use.
    pub fn ensure(
        &mut self,
        device: &wgpu::Device,
        effects: &EffectRegistry,
        key: &PipelineKey,
    ) -> u32 {
        if let Some(index) = self.find(key) {
            return index;
        }
        let shader = key.shader;
        let module = self.modules.entry(shader).or_insert_with(|| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(match shader.pass {
                    Pass::Main => "surface",
                    Pass::Shadow => "shadow",
                }),
                source: wgpu::ShaderSource::Wgsl(shader_source(&shader, effects).into()),
            })
        });
        let extra = wgpu::VertexBufferLayout {
            array_stride: 16 * shader.extra_attributes as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &EXTRA_ATTRIBUTES[..shader.extra_attributes as usize],
        };
        let buffers = [
            Some(vertex_layout()),
            (shader.extra_attributes > 0).then_some(extra),
        ];
        let buffers = &buffers[..1 + (shader.extra_attributes > 0) as usize];
        let main = shader.pass == Pass::Main;
        let targets = [Some(wgpu::ColorTargetState {
            format: HDR_FORMAT,
            blend: blend_state(key.blend),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let fragment = if main {
            Some(wgpu::FragmentState {
                module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &targets,
            })
        } else if shader.shadow_needs_fragment(effects) {
            Some(wgpu::FragmentState {
                module,
                entry_point: Some("fs_shadow"),
                compilation_options: Default::default(),
                targets: &[],
            })
        } else {
            None
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(if main { "surface" } else { "shadow" }),
            layout: Some(&self.surface_layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some(if main { "vs_main" } else { "vs_shadow" }),
                compilation_options: Default::default(),
                buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: cull_mode(key.side),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(key.depth_write),
                depth_compare: Some(if key.depth_test {
                    wgpu::CompareFunction::LessEqual
                } else {
                    wgpu::CompareFunction::Always
                }),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: key.depth_bias.constant,
                    slope_scale: key.depth_bias.slope_scale(),
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState {
                count: if main { SAMPLE_COUNT } else { 1 },
                mask: !0,
                alpha_to_coverage_enabled: key.alpha_to_coverage,
            },
            fragment,
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.push(pipeline);
        self.created += 1;
        let index = self.pipelines.len() as u32 - 1;
        self.index.insert(*key, index);
        index
    }

    /// Pipelines including the fixed water and output ones.
    pub fn count(&self) -> usize {
        self.pipelines.len() + 2
    }

    pub fn module_count(&self) -> usize {
        self.modules.len() + 2
    }
}
