//! Where the vertex stage reads instance records: the view's records and each
//! effect pool's. WebGPU keeps them in a storage buffer. WebGL2 has no storage
//! buffers, so the `webgl` build keeps them in an RGBA32F texture instead, one texel
//! per vec4 and `RECORDS_PER_ROW` records per row, which `instances_texture.wgsl`
//! reads with `textureLoad`. Either way a store has a fixed capacity, and writes
//! upload only the records they cover.

use crate::draw_list::InstanceRecord;
#[cfg(feature = "webgl")]
use crate::draw_list::{RECORD_TEXELS, RECORDS_PER_ROW};

use super::RECORD_SIZE;

/// Frame group binding 5.
pub const INSTANCE_ENTRY: wgpu::BindGroupLayoutEntry = wgpu::BindGroupLayoutEntry {
    binding: 5,
    visibility: wgpu::ShaderStages::VERTEX,
    #[cfg(not(feature = "webgl"))]
    ty: wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Storage { read_only: true },
        has_dynamic_offset: false,
        min_binding_size: None,
    },
    #[cfg(feature = "webgl")]
    ty: wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable: false },
        view_dimension: wgpu::TextureViewDimension::D2,
        multisampled: false,
    },
    count: None,
};

pub struct InstanceStore {
    #[cfg(not(feature = "webgl"))]
    buffer: wgpu::Buffer,
    #[cfg(feature = "webgl")]
    texture: wgpu::Texture,
    #[cfg(feature = "webgl")]
    view: wgpu::TextureView,
    capacity: u32,
}

impl InstanceStore {
    #[cfg(not(feature = "webgl"))]
    pub fn new(device: &wgpu::Device, label: &str, capacity: u32) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: capacity as u64 * RECORD_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buffer, capacity }
    }

    /// Whole rows: the capacity rounds up to a multiple of `RECORDS_PER_ROW`.
    #[cfg(feature = "webgl")]
    pub fn new(device: &wgpu::Device, label: &str, capacity: u32) -> Self {
        let rows = capacity.div_ceil(RECORDS_PER_ROW).max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: RECORDS_PER_ROW * RECORD_TEXELS,
                height: rows,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        Self {
            view: texture.create_view(&Default::default()),
            texture,
            capacity: rows * RECORDS_PER_ROW,
        }
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn bytes(&self) -> u64 {
        self.capacity as u64 * RECORD_SIZE
    }

    pub fn binding(&self) -> wgpu::BindingResource<'_> {
        #[cfg(not(feature = "webgl"))]
        return self.buffer.as_entire_binding();
        #[cfg(feature = "webgl")]
        return wgpu::BindingResource::TextureView(&self.view);
    }

    pub fn destroy(&self) {
        #[cfg(not(feature = "webgl"))]
        self.buffer.destroy();
        #[cfg(feature = "webgl")]
        self.texture.destroy();
    }

    /// Upload `records` from record `first` on; the caller keeps them in capacity.
    #[cfg(not(feature = "webgl"))]
    pub fn write(&self, queue: &wgpu::Queue, first: u32, records: &[InstanceRecord]) {
        queue.write_buffer(
            &self.buffer,
            first as u64 * RECORD_SIZE,
            bytemuck::cast_slice(records),
        );
    }

    /// Upload `records` from record `first` on as at most three rectangles: the
    /// rest of the first row, the whole rows after it and the start of the last.
    #[cfg(feature = "webgl")]
    pub fn write(&self, queue: &wgpu::Queue, first: u32, records: &[InstanceRecord]) {
        let mut first = first;
        let mut records = records;
        while !records.is_empty() {
            let column = first % RECORDS_PER_ROW;
            let (width, rows) = if column != 0 || (records.len() as u32) < RECORDS_PER_ROW {
                let width = (RECORDS_PER_ROW - column).min(records.len() as u32);
                (width, 1)
            } else {
                (RECORDS_PER_ROW, records.len() as u32 / RECORDS_PER_ROW)
            };
            let count = (width * rows) as usize;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: column * RECORD_TEXELS,
                        y: first / RECORDS_PER_ROW,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&records[..count]),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * RECORD_SIZE as u32),
                    rows_per_image: Some(rows),
                },
                wgpu::Extent3d {
                    width: width * RECORD_TEXELS,
                    height: rows,
                    depth_or_array_layers: 1,
                },
            );
            first += count as u32;
            records = &records[count..];
        }
    }
}
