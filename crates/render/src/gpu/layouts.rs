/// Bind group layouts shared by every pipeline.
pub struct Layouts {
    pub frame: wgpu::BindGroupLayout,
    pub material: wgpu::BindGroupLayout,
    pub water: wgpu::BindGroupLayout,
    pub output: wgpu::BindGroupLayout,
}

const fn texture_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    staged_texture_entry(binding, filterable, wgpu::ShaderStages::FRAGMENT)
}

const fn staged_texture_entry(
    binding: u32,
    filterable: bool,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

const fn sampler_entry(binding: u32, ty: wgpu::SamplerBindingType) -> wgpu::BindGroupLayoutEntry {
    staged_sampler_entry(binding, ty, wgpu::ShaderStages::FRAGMENT)
}

const fn staged_sampler_entry(
    binding: u32,
    ty: wgpu::SamplerBindingType,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Sampler(ty),
        count: None,
    }
}

const fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

// The entries are data so the pipeline precompiler (`precompile.rs`) builds the
// same layouts as wgpu.
use wgpu::SamplerBindingType::{Comparison, Filtering};

/// Frame uniforms, sun shadow map, reflection and the instance records.
pub const FRAME_ENTRIES: &[wgpu::BindGroupLayoutEntry] = &[
    uniform_entry(0),
    wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    },
    sampler_entry(2, Comparison),
    texture_entry(3, true),
    sampler_entry(4, Filtering),
    super::super::instance_store::INSTANCE_ENTRY,
];

/// A uniform block and two filtered textures (the water).
pub const TEXTURED_ENTRIES: &[wgpu::BindGroupLayoutEntry] = &[
    uniform_entry(0),
    texture_entry(1, true),
    sampler_entry(2, Filtering),
    texture_entry(3, true),
    sampler_entry(4, Filtering),
];

pub const OUTPUT_ENTRIES: &[wgpu::BindGroupLayoutEntry] =
    &[texture_entry(0, false), uniform_entry(1)];

/// Map, bump and emissive map for the surface; effect textures for any stage.
pub const MATERIAL_ENTRIES: &[wgpu::BindGroupLayoutEntry] = &[
    uniform_entry(0),
    texture_entry(1, true),
    sampler_entry(2, Filtering),
    texture_entry(3, true),
    sampler_entry(4, Filtering),
    texture_entry(5, true),
    sampler_entry(6, Filtering),
    staged_texture_entry(7, true, wgpu::ShaderStages::VERTEX_FRAGMENT),
    staged_sampler_entry(8, Filtering, wgpu::ShaderStages::VERTEX_FRAGMENT),
    staged_texture_entry(9, true, wgpu::ShaderStages::VERTEX_FRAGMENT),
    staged_sampler_entry(10, Filtering, wgpu::ShaderStages::VERTEX_FRAGMENT),
];

impl Layouts {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = |label, entries| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        Self {
            frame: layout("frame", FRAME_ENTRIES),
            material: layout("material", MATERIAL_ENTRIES),
            water: layout("water", TEXTURED_ENTRIES),
            output: layout("output", OUTPUT_ENTRIES),
        }
    }
}
