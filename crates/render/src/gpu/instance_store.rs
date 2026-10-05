//! WebGPU instance records, stored in a fixed-capacity storage buffer.
use super::RECORD_SIZE;
use crate::draw_list::InstanceRecord;
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
    pub fn new(device: &wgpu::Device, label: &str, capacity: u32) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
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
    pub fn bytes(&self) -> u64 {
        self.capacity as u64 * RECORD_SIZE
    }
    pub fn binding(&self) -> wgpu::BindingResource<'_> {
        self.buffer.as_entire_binding()
    }
    pub fn destroy(&self) {
        self.buffer.destroy();
    }
    pub fn write(&self, queue: &wgpu::Queue, first: u32, records: &[InstanceRecord]) {
        queue.write_buffer(
            &self.buffer,
            first as u64 * RECORD_SIZE,
            bytemuck::cast_slice(records),
        );
    }
}
