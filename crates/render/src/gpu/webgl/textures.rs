//! WebGL textures: `texSubImage2D` uploads straight from the decoded `ImageBitmap`
//! or generated `ImageData` into immutable storage, mip levels rendered from the
//! level above by the same blit as WebGPU's (`mipmap.wgsl`; drawing into an sRGB
//! level averages in linear space), and sampler objects.

use glow::HasContext;
use sloppy_core::scene::Wrap;

use super::context::{Gpu, Raster, unit};
use super::programs::{Linking, Program};
use crate::gpu::textures::{Pixels, SamplerKey, TextureKey};
use crate::shader::MIPMAP_WGSL;

pub type TextureView = glow::Texture;
pub type Sampler = glow::Sampler;

/// An uploaded texture with its mip chain; dropping it deletes it.
pub struct Texture {
    gpu: Gpu,
    pub view: glow::Texture,
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe { self.gpu.gl.delete_texture(self.view) };
    }
}

/// A linear clamp-to-edge sampler, or with `compare` a depth comparison one.
pub fn linear_sampler(gpu: &Gpu, compare: bool) -> glow::Sampler {
    let gl = &gpu.gl;
    unsafe {
        let sampler = gpu.created(gl.create_sampler(), "sampler");
        for (name, value) in [
            (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
            (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.sampler_parameter_i32(sampler, name, value as i32);
        }
        if compare {
            gl.sampler_parameter_i32(
                sampler,
                glow::TEXTURE_COMPARE_MODE,
                glow::COMPARE_REF_TO_TEXTURE as i32,
            );
            gl.sampler_parameter_i32(sampler, glow::TEXTURE_COMPARE_FUNC, glow::LEQUAL as i32);
        }
        sampler
    }
}

/// Immutable storage for a `width` × `height` texture of `levels` levels.
pub fn storage(gpu: &Gpu, format: u32, levels: u32, width: u32, height: u32) -> glow::Texture {
    let texture = gpu.created(unsafe { gpu.gl.create_texture() }, "texture");
    gpu.bind_upload_texture(texture);
    unsafe {
        gpu.gl.tex_storage_2d(
            glow::TEXTURE_2D,
            levels as i32,
            format,
            width as i32,
            height as i32,
        )
    };
    texture
}

/// The mip blit's program while it links, then linked.
enum Blit {
    Linking(Linking),
    Ready(Program),
}

/// Uploads textures and renders their mip levels; holds the white placeholder that
/// materials sample until their textures arrive.
pub struct Uploader {
    gpu: Gpu,
    blit: Option<Blit>,
    framebuffer: glow::Framebuffer,
    /// Full-screen triangles read no attributes.
    no_attributes: glow::VertexArray,
    sampler: glow::Sampler,
    placeholder: glow::Texture,
}

impl Uploader {
    /// Starts linking the mip blit; [`Self::ready`] reports once it has.
    pub fn new(gpu: &Gpu) -> Self {
        let blit = Linking::start(gpu, "mipmap", MIPMAP_WGSL, "vs_blit", Some("fs_blit"));
        let placeholder = storage(gpu, glow::RGBA8, 1, 1, 1);
        gpu.set_flip_y(false);
        unsafe {
            gpu.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                1,
                1,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&[255; 4])),
            );
        }
        Self {
            gpu: gpu.clone(),
            blit: Some(Blit::Linking(blit)),
            framebuffer: gpu.created(unsafe { gpu.gl.create_framebuffer() }, "framebuffer"),
            no_attributes: gpu.created(unsafe { gpu.gl.create_vertex_array() }, "vertex array"),
            sampler: linear_sampler(gpu, false),
            placeholder,
        }
    }

    /// Whether the mip blit has linked; uploads wait until then.
    pub fn ready(&mut self, gpu: &Gpu) -> bool {
        if let Some(Blit::Linking(linking)) = &self.blit
            && linking.done(gpu)
            && let Some(Blit::Linking(linking)) = self.blit.take()
        {
            self.blit = Some(Blit::Ready(linking.finish(gpu)));
        }
        matches!(self.blit, Some(Blit::Ready(_)))
    }

    /// The white placeholder.
    pub fn placeholder(&self) -> &glow::Texture {
        &self.placeholder
    }

    /// Upload `pixels` as a texture with `levels` mip levels; flips rows while
    /// copying when `flip_y` (`ImageData` only: bitmaps are flipped when decoded).
    pub fn upload(
        &self,
        gpu: &Gpu,
        key: &TextureKey,
        pixels: &Pixels,
        levels: u32,
        flip_y: bool,
    ) -> Texture {
        let (width, height) = pixels.size();
        let format = if key.srgb {
            glow::SRGB8_ALPHA8
        } else {
            glow::RGBA8
        };
        let texture = storage(gpu, format, levels, width, height);
        let gl = &gpu.gl;
        unsafe {
            match pixels {
                Pixels::Bitmap(bitmap) => {
                    gpu.set_flip_y(false);
                    gl.tex_sub_image_2d_with_image_bitmap(
                        glow::TEXTURE_2D,
                        0,
                        0,
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        bitmap,
                    );
                }
                Pixels::Image(image) => {
                    gpu.set_flip_y(flip_y);
                    gl.tex_sub_image_2d_with_image_data(
                        glow::TEXTURE_2D,
                        0,
                        0,
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        image,
                    );
                }
            }
        }
        self.generate_mipmaps(gpu, texture, levels, width, height);
        Texture {
            gpu: gpu.clone(),
            view: texture,
        }
    }

    /// Render levels 1.. each from the level above, sampling only that level so the
    /// level drawn into is never one being read.
    fn generate_mipmaps(
        &self,
        gpu: &Gpu,
        texture: glow::Texture,
        levels: u32,
        width: u32,
        height: u32,
    ) {
        if levels < 2 {
            return;
        }
        let Some(Blit::Ready(program)) = &self.blit else {
            unreachable!("uploads wait for the mip blit");
        };
        let gl = &gpu.gl;
        gpu.bind_framebuffer(Some(self.framebuffer));
        gpu.use_program(program.raw);
        gpu.set_raster(&Raster::PLAIN);
        gpu.bind_vertex_array(Some(self.no_attributes));
        gpu.bind_texture(unit::SOURCE, texture);
        gpu.bind_sampler(unit::SOURCE, Some(self.sampler));
        let level_range = |base: u32, max: u32| {
            gpu.bind_upload_texture(texture);
            unsafe {
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_BASE_LEVEL, base as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAX_LEVEL, max as i32);
            }
        };
        for level in 1..levels {
            level_range(level - 1, level - 1);
            unsafe {
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(texture),
                    level as i32,
                );
            }
            gpu.viewport((width >> level).max(1), (height >> level).max(1));
            unsafe { gl.draw_arrays(glow::TRIANGLES, 0, 3) };
        }
        level_range(0, levels - 1);
        unsafe {
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                None,
                0,
            );
        }
    }

    /// A material sampler (`webgpu/textures.rs` `Uploader::sampler`).
    pub fn sampler(gpu: &Gpu, key: SamplerKey) -> glow::Sampler {
        let gl = &gpu.gl;
        let wrap = match key.wrap {
            Wrap::Clamp => glow::CLAMP_TO_EDGE,
            Wrap::Repeat => glow::REPEAT,
            Wrap::Mirror => glow::MIRRORED_REPEAT,
        };
        // Anisotropy requires linear mip filtering even for one level.
        let min_filter = if key.mipmaps || key.anisotropy > 1 {
            glow::LINEAR_MIPMAP_LINEAR
        } else {
            glow::LINEAR_MIPMAP_NEAREST
        };
        unsafe {
            let sampler = gpu.created(gl.create_sampler(), "sampler");
            for (name, value) in [
                (glow::TEXTURE_MIN_FILTER, min_filter),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, wrap),
                (glow::TEXTURE_WRAP_T, wrap),
            ] {
                gl.sampler_parameter_i32(sampler, name, value as i32);
            }
            if key.anisotropy > 1 && gpu.anisotropy {
                gl.sampler_parameter_f32(
                    sampler,
                    glow::TEXTURE_MAX_ANISOTROPY_EXT,
                    f32::from(key.anisotropy),
                );
            }
            sampler
        }
    }
}

impl Drop for Uploader {
    fn drop(&mut self) {
        let gl = &self.gpu.gl;
        unsafe {
            gl.delete_framebuffer(self.framebuffer);
            gl.delete_vertex_array(self.no_attributes);
            gl.delete_sampler(self.sampler);
            gl.delete_texture(self.placeholder);
        }
    }
}
