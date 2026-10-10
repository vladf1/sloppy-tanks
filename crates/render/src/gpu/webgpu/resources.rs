//! WebGPU bind group layouts, mesh page buffers and material bind groups.

use super::Gpu;
use crate::gpu::resources::{MATERIAL_PARAMS_OFFSET, MaterialTextures, MaterialUniform};
use crate::mesh_pages::PageFamily;

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
    super::instance_store::INSTANCE_ENTRY,
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

/// The mipmap blit's source level and sampler.
pub const MIPMAP_SOURCE_ENTRIES: &[wgpu::BindGroupLayoutEntry] =
    &[texture_entry(0, true), sampler_entry(1, Filtering)];

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

pub fn uniform_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// A mesh page's buffer: its vertices or indices. Draws bind it only through
/// [`Self::vertex_buffers`] and [`Self::index_buffer`], which bind the written
/// prefix: never `slice(..)` a page for a draw, or wgpu clears its unwritten tail
/// first. Dropping a page destroys its buffer.
pub struct PageBuffers {
    main: wgpu::Buffer,
}

impl PageBuffers {
    /// A buffer for `capacity` elements of `family`. Pages are filled only through
    /// the queue, never mapped at creation: the browser backend stages a mapped range
    /// in a Wasm-side copy of the whole buffer, and linear memory never shrinks.
    pub fn new(gpu: &Gpu, family: PageFamily, capacity: u32) -> Self {
        let (label, usage) = match family {
            PageFamily::Surface => ("mesh vertex page", wgpu::BufferUsages::VERTEX),
            PageFamily::Shadow => ("shadow vertex page", wgpu::BufferUsages::VERTEX),
            PageFamily::Index => ("mesh index page", wgpu::BufferUsages::INDEX),
        };
        Self {
            main: gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: u64::from(capacity) * family.stride(),
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }

    /// Write `data` at byte `offset`.
    pub fn write(&self, gpu: &Gpu, offset: u64, data: &[u8]) {
        gpu.queue.write_buffer(&self.main, offset, data);
    }

    /// A vertex page to bind: only the prefix of `written` elements. Every placed
    /// range is written before anything can draw from it and a freed range keeps its
    /// old contents, so the prefix holds every mesh in the page and is always
    /// initialized. Binding the never-written tail as well would make wgpu zero-fill it
    /// before the pass.
    pub fn vertex_buffers(&self, family: PageFamily, written: u32) -> wgpu::BufferSlice<'_> {
        self.main.slice(..u64::from(written) * family.stride())
    }

    /// An index page to bind: only its written prefix, as in
    /// [`vertex_buffers`](Self::vertex_buffers).
    pub fn index_buffer(&self, written: u32) -> wgpu::BufferSlice<'_> {
        self.main.slice(..u64::from(written) * 4)
    }
}

impl Drop for PageBuffers {
    fn drop(&mut self) {
        self.main.destroy();
    }
}

/// A material's uniform buffer and its bind group (uniform, then a texture and
/// sampler per slot: map, bump, emissive, effect extras). Dropping it destroys the
/// uniform buffer.
pub struct MaterialBinding {
    uniform: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
}

impl MaterialBinding {
    pub fn new(gpu: &Gpu, uniform: &MaterialUniform, textures: MaterialTextures) -> Self {
        let buffer = uniform_buffer(
            &gpu.device,
            "material uniform",
            size_of::<MaterialUniform>() as u64,
        );
        gpu.queue
            .write_buffer(&buffer, 0, bytemuck::bytes_of(uniform));
        let bind_group = Self::bind_group(gpu, &buffer, textures);
        Self {
            uniform: buffer,
            bind_group,
        }
    }

    fn bind_group(
        gpu: &Gpu,
        uniform: &wgpu::Buffer,
        textures: MaterialTextures,
    ) -> wgpu::BindGroup {
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }];
        for (slot, (view, sampler)) in textures.iter().enumerate() {
            let binding = 1 + slot as u32 * 2;
            entries.push(wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::TextureView(view),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: binding + 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            });
        }
        gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material"),
            layout: &gpu.layouts.material,
            entries: &entries,
        })
    }

    /// Bind other textures (ones that arrived since).
    pub fn rebind(&mut self, gpu: &Gpu, textures: MaterialTextures) {
        self.bind_group = Self::bind_group(gpu, &self.uniform, textures);
    }

    /// Overwrite the uniform's 16 effect params.
    pub fn write_params(&self, gpu: &Gpu, params: &[[f32; 4]; 4]) {
        gpu.queue.write_buffer(
            &self.uniform,
            MATERIAL_PARAMS_OFFSET,
            bytemuck::cast_slice(params),
        );
    }
}

impl Drop for MaterialBinding {
    fn drop(&mut self) {
        self.uniform.destroy();
    }
}
