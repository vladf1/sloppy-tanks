//! The WebGL build's copy of the cached fixed-scenery shadow into the sun shadow map.
//! WebGPU copies the depth texture (`copy_texture_to_texture`); WebGL2 cannot copy
//! depth, so a full-screen triangle writes every texel's depth instead, at the start
//! of the sun shadow pass. It moves the same texels as the copy, so the cache stays
//! cheaper than redrawing the scenery it holds.

use super::context::DEPTH_FORMAT;
use crate::shader::DEPTH_COPY_WGSL;

const SOURCE_ENTRIES: &[wgpu::BindGroupLayoutEntry] = &[
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    },
    wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
        count: None,
    },
];

pub struct DepthCopy {
    layout: wgpu::BindGroupLayout,
    /// Nearest filtering, which also makes the depth texture complete for GLSL ES.
    sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    /// Reads the current cached shadow (`retarget` after it is recreated).
    group: wgpu::BindGroup,
}

impl DepthCopy {
    pub fn new(device: &wgpu::Device, source: &wgpu::TextureView) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("depth copy"),
            entries: SOURCE_ENTRIES,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("depth copy"),
            source: wgpu::ShaderSource::Wgsl(DEPTH_COPY_WGSL.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("depth copy"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("depth copy"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_fullscreen"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_copy_depth"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("depth copy"),
            ..Default::default()
        });
        let group = Self::source_group(device, &layout, &sampler, source);
        Self {
            layout,
            sampler,
            pipeline,
            group,
        }
    }

    fn source_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        source: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("depth copy"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// Read `source` from now on.
    pub fn retarget(&mut self, device: &wgpu::Device, source: &wgpu::TextureView) {
        self.group = Self::source_group(device, &self.layout, &self.sampler, source);
    }

    /// Overwrite the pass's whole depth attachment with the source's depth. Encode it
    /// first: it replaces the pass's bind group 0.
    pub fn encode(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}
