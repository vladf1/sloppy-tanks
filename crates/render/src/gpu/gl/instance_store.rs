//! Fixed-capacity instance textures. Row packing is shared with WGSL and WebGPU's
//! draw-list records; uploads never allocate a texture-sized Wasm staging buffer.
use crate::draw_list::{InstanceRecord, RECORD_TEXELS, RECORDS_PER_ROW, instance_texture_regions};
use crate::gpu::gl::device::{Device, Queue, Texture};
use glow::HasContext;
pub struct InstanceStore {
    pub texture: Texture,
    capacity: u32,
}
impl InstanceStore {
    pub fn new(device: &Device, _label: &str, capacity: u32) -> Self {
        let rows = capacity.div_ceil(RECORDS_PER_ROW).max(1);
        Self {
            texture: Texture::new(
                device,
                glow::RGBA32F,
                RECORDS_PER_ROW * RECORD_TEXELS,
                rows,
                1,
            ),
            capacity: rows * RECORDS_PER_ROW,
        }
    }
    pub fn capacity(&self) -> u32 {
        self.capacity
    }
    pub fn bytes(&self) -> u64 {
        self.capacity as u64 * crate::gpu::RECORD_SIZE
    }
    pub fn destroy(&self) {
        self.texture.destroy();
    }
    pub fn write(&self, queue: &Queue, first: u32, mut records: &[InstanceRecord]) {
        queue.upload(&self.texture);
        for region in instance_texture_regions(first, records.len() as u32) {
            let column = region.column;
            let width = region.width;
            let rows = region.rows;
            let count = region.records() as usize;
            unsafe {
                queue.gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    (column * RECORD_TEXELS) as i32,
                    region.row as i32,
                    (width * RECORD_TEXELS) as i32,
                    rows as i32,
                    glow::RGBA,
                    glow::FLOAT,
                    glow::PixelUnpackData::Slice(Some(bytemuck::cast_slice(&records[..count]))),
                );
            }
            records = &records[count..];
        }
    }
}
