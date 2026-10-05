//! WebGL mesh page buffers, material bindings and instance record stores. Each
//! deletes its GL objects when dropped.

use std::cell::Cell;

use glow::{HasContext, PixelUnpackData};

use super::context::{Gpu, block, unit};
use super::textures::{Sampler, TextureView};
use crate::draw_list::{InstanceRecord, RECORD_TEXELS, RECORDS_PER_ROW};
use crate::gpu::RECORD_SIZE;
use crate::gpu::resources::{EXTRA_TEXTURE_SLOTS, MATERIAL_PARAMS_OFFSET, MaterialUniform};
use crate::mesh_pages::PageFamily;
use crate::model::Vertex;
use crate::shadow_merge::ShadowVertex;

/// The merged casters' per-instance record base (`shadow_merged.wgsl` `base`).
pub const SHADOW_BASE_LOCATION: u32 = 2;

/// A mesh page's buffers: its vertices or indices, and for a surface page with
/// effect vec4s those, in the same vertex numbering. A vertex page also owns the
/// vertex array that reads it, so switching pages is one binding; its element
/// buffer, vertex-array state, is the index page it last drew from.
pub struct PageBuffers {
    gpu: Gpu,
    main: glow::Buffer,
    extra: Option<glow::Buffer>,
    vertex_array: Option<glow::VertexArray>,
    bound_indices: Cell<Option<glow::Buffer>>,
}

/// `vertexAttribPointer` for a float vector at `location`.
fn float_attribute(gpu: &Gpu, location: u32, size: i32, stride: usize, offset: usize) {
    unsafe {
        gpu.gl.vertex_attrib_pointer_f32(
            location,
            size,
            glow::FLOAT,
            false,
            stride as i32,
            offset as i32,
        );
        gpu.gl.enable_vertex_attrib_array(location);
    }
}

impl PageBuffers {
    /// Buffers for `capacity` elements of `family`, zeroed by the browser.
    pub fn new(gpu: &Gpu, family: PageFamily, capacity: u32) -> Self {
        let bytes = |stride: u64| u64::from(capacity) * stride;
        if family == PageFamily::Index {
            return Self {
                gpu: gpu.clone(),
                main: gpu.create_buffer(glow::ELEMENT_ARRAY_BUFFER, bytes(4), glow::STATIC_DRAW),
                extra: None,
                vertex_array: None,
                bound_indices: Cell::new(None),
            };
        }
        let main = gpu.create_buffer(
            glow::ARRAY_BUFFER,
            bytes(family.stride()),
            glow::STATIC_DRAW,
        );
        let extra = (family.extra_stride() > 0).then(|| {
            gpu.create_buffer(
                glow::ARRAY_BUFFER,
                bytes(family.extra_stride()),
                glow::STATIC_DRAW,
            )
        });
        let vertex_array = gpu.created(unsafe { gpu.gl.create_vertex_array() }, "vertex array");
        gpu.bind_vertex_array(Some(vertex_array));
        gpu.bind_array_buffer(main);
        match family {
            PageFamily::Surface { extra: count } => {
                let stride = size_of::<Vertex>();
                float_attribute(gpu, 0, 3, stride, 0);
                float_attribute(gpu, 1, 3, stride, 12);
                float_attribute(gpu, 2, 2, stride, 24);
                float_attribute(gpu, 3, 4, stride, 32);
                if let Some(extra) = extra {
                    gpu.bind_array_buffer(extra);
                    for slot in 0..u32::from(count) {
                        float_attribute(gpu, 4 + slot, 4, 16 * count as usize, 16 * slot as usize);
                    }
                }
            }
            PageFamily::Shadow => {
                let stride = size_of::<ShadowVertex>();
                float_attribute(gpu, 0, 3, stride, 0);
                unsafe {
                    gpu.gl
                        .vertex_attrib_pointer_i32(1, 1, glow::UNSIGNED_INT, stride as i32, 12);
                    gpu.gl.enable_vertex_attrib_array(1);
                    // Each draw points it into the frame's record bases.
                    gpu.gl.enable_vertex_attrib_array(SHADOW_BASE_LOCATION);
                    gpu.gl.vertex_attrib_divisor(SHADOW_BASE_LOCATION, 1);
                }
                float_attribute(gpu, 3, 2, stride, 16);
            }
            PageFamily::Index => unreachable!("index pages have no vertex array"),
        }
        Self {
            gpu: gpu.clone(),
            main,
            extra,
            vertex_array: Some(vertex_array),
            bound_indices: Cell::new(None),
        }
    }

    /// Write `data` at byte `offset` of the main buffer.
    pub fn write(&self, gpu: &Gpu, offset: u64, data: &[u8]) {
        gpu.write_buffer(self.main, offset, data);
    }

    /// Write effect vec4s at byte `offset` of the effect attribute buffer, if any.
    pub fn write_extra(&self, gpu: &Gpu, offset: u64, data: &[u8]) {
        if let Some(extra) = self.extra {
            gpu.write_buffer(extra, offset, data);
        }
    }

    /// Bind this vertex page's vertex array, drawing indices from `indices`.
    pub fn bind(&self, gpu: &Gpu, indices: &PageBuffers) {
        gpu.bind_vertex_array(self.vertex_array);
        if self.bound_indices.replace(Some(indices.main)) != Some(indices.main) {
            unsafe {
                gpu.gl
                    .bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(indices.main))
            };
        }
    }
}

impl Drop for PageBuffers {
    fn drop(&mut self) {
        let gl = &self.gpu.gl;
        unsafe {
            if let Some(vertex_array) = self.vertex_array {
                gl.delete_vertex_array(vertex_array);
            }
            gl.delete_buffer(self.main);
            if let Some(extra) = self.extra {
                gl.delete_buffer(extra);
            }
        }
    }
}

/// A material's textures and their samplers, in binding order.
pub type MaterialTextures<'a> = [(&'a TextureView, Sampler); 3 + EXTRA_TEXTURE_SLOTS];

/// A material's uniform buffer and its textures (map, bump, emissive, effect
/// extras), bound to the material block and units.
pub struct MaterialBinding {
    gpu: Gpu,
    uniform: glow::Buffer,
    textures: [(glow::Texture, glow::Sampler); 3 + EXTRA_TEXTURE_SLOTS],
}

impl MaterialBinding {
    pub fn new(gpu: &Gpu, uniform: &MaterialUniform, textures: MaterialTextures) -> Self {
        let buffer = gpu.create_buffer(
            glow::UNIFORM_BUFFER,
            size_of::<MaterialUniform>() as u64,
            glow::DYNAMIC_DRAW,
        );
        gpu.write_buffer(buffer, 0, bytemuck::bytes_of(uniform));
        Self {
            gpu: gpu.clone(),
            uniform: buffer,
            textures: textures.map(|(view, sampler)| (*view, sampler)),
        }
    }

    /// Bind other textures (ones that arrived since).
    pub fn rebind(&mut self, _gpu: &Gpu, textures: MaterialTextures) {
        self.textures = textures.map(|(view, sampler)| (*view, sampler));
    }

    /// Overwrite the uniform's 16 effect params.
    pub fn write_params(&self, gpu: &Gpu, params: &[[f32; 4]; 4]) {
        gpu.write_buffer(
            self.uniform,
            MATERIAL_PARAMS_OFFSET,
            bytemuck::cast_slice(params),
        );
    }

    /// Bind the uniform and textures for the next draws.
    pub fn bind(&self, gpu: &Gpu) {
        gpu.bind_uniform_block(block::MATERIAL, self.uniform);
        for (slot, &(texture, sampler)) in self.textures.iter().enumerate() {
            let unit = unit::MATERIAL + slot as u32;
            gpu.bind_texture(unit, texture);
            gpu.bind_sampler(unit, Some(sampler));
        }
    }
}

impl Drop for MaterialBinding {
    fn drop(&mut self) {
        unsafe { self.gpu.gl.delete_buffer(self.uniform) };
    }
}

/// Instance records in an RGBA32F texture (WebGL2 has no storage buffers), one
/// texel per vec4 and `RECORDS_PER_ROW` records per row, which
/// `instances_texture.wgsl` reads with `texelFetch`. A store has a fixed capacity,
/// rounded up to whole rows, and writes upload only the records they cover.
pub struct InstanceStore {
    gpu: Gpu,
    texture: glow::Texture,
    capacity: u32,
}

impl InstanceStore {
    pub fn new(gpu: &Gpu, _label: &str, capacity: u32) -> Self {
        let rows = capacity.div_ceil(RECORDS_PER_ROW).max(1);
        let texture = gpu.created(unsafe { gpu.gl.create_texture() }, "texture");
        gpu.bind_upload_texture(texture);
        unsafe {
            let gl = &gpu.gl;
            gl.tex_storage_2d(
                glow::TEXTURE_2D,
                1,
                glow::RGBA32F,
                (RECORDS_PER_ROW * RECORD_TEXELS) as i32,
                rows as i32,
            );
            // Read with `texelFetch` and no sampler: complete only with nearest
            // filtering, as float textures are not filterable.
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
        }
        Self {
            gpu: gpu.clone(),
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

    pub fn texture(&self) -> glow::Texture {
        self.texture
    }

    /// Upload `records` from record `first` on as at most three rectangles: the
    /// rest of the first row, the whole rows after it and the start of the last.
    pub fn write(&self, gpu: &Gpu, first: u32, records: &[InstanceRecord]) {
        let mut first = first;
        let mut records = records;
        if records.is_empty() {
            return;
        }
        gpu.bind_upload_texture(self.texture);
        gpu.set_flip_y(false);
        while !records.is_empty() {
            let column = first % RECORDS_PER_ROW;
            let (width, rows) = if column != 0 || (records.len() as u32) < RECORDS_PER_ROW {
                let width = (RECORDS_PER_ROW - column).min(records.len() as u32);
                (width, 1)
            } else {
                (RECORDS_PER_ROW, records.len() as u32 / RECORDS_PER_ROW)
            };
            let count = (width * rows) as usize;
            unsafe {
                gpu.gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    (column * RECORD_TEXELS) as i32,
                    (first / RECORDS_PER_ROW) as i32,
                    (width * RECORD_TEXELS) as i32,
                    rows as i32,
                    glow::RGBA,
                    glow::FLOAT,
                    PixelUnpackData::Slice(Some(bytemuck::cast_slice(&records[..count]))),
                );
            }
            first += count as u32;
            records = &records[count..];
        }
    }
}

impl Drop for InstanceStore {
    fn drop(&mut self) {
        unsafe { self.gpu.gl.delete_texture(self.texture) };
    }
}
