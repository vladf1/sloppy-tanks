//! Where the vertex stage reads instance records: the view's records and each
//! effect pool's, in a storage buffer (`instances_storage.wgsl`). A store has a fixed
//! capacity, writes upload only the records they cover, and dropping it destroys the
//! buffer.

use super::Gpu;
use crate::draw_list::InstanceRecord;
use crate::gpu::RECORD_SIZE;

/// Frame group binding 5.
pub const INSTANCE_ENTRY: wgpu::BindGroupLayoutEntry = wgpu::BindGroupLayoutEntry {
    binding: 5,
    visibility: wgpu::ShaderStages::VERTEX,
    ty: wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Storage { read_only: true },
        has_dynamic_offset: false,
        min_binding_size: None,
    },
    count: None,
};

pub struct InstanceStore {
    buffer: wgpu::Buffer,
    capacity: u32,
}

impl InstanceStore {
    pub fn new(gpu: &Gpu, label: &str, capacity: u32) -> Self {
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: capacity as u64 * RECORD_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buffer, capacity }
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn binding(&self) -> wgpu::BindingResource<'_> {
        self.buffer.as_entire_binding()
    }

    /// Upload `records` from record `first` on; the caller keeps them in capacity.
    pub fn write(&self, gpu: &Gpu, first: u32, records: &[InstanceRecord]) {
        gpu.queue.write_buffer(
            &self.buffer,
            first as u64 * RECORD_SIZE,
            bytemuck::cast_slice(records),
        );
    }
}

impl Drop for InstanceStore {
    fn drop(&mut self) {
        self.buffer.destroy();
    }
}
