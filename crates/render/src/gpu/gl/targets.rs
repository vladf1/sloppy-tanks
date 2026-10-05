//! Owned framebuffer attachments. Resize replaces and deletes the old storage;
//! binding changes and deletion always update the context's state cache.
use super::device::{Device, Texture};
use glow::HasContext;
use std::cell::Cell;

pub struct DepthTarget {
    device: Device,
    pub texture: Texture,
    framebuffer: Cell<Option<glow::Framebuffer>>,
    pub size: u32,
}
impl DepthTarget {
    pub fn new(device: &Device, size: u32) -> Self {
        unsafe {
            let texture = Texture::new(device, glow::DEPTH_COMPONENT32F, size, size, 1);
            let framebuffer = device.gl.create_framebuffer().expect("depth framebuffer");
            device.target(Some(framebuffer), size, size);
            device.gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::TEXTURE_2D,
                Some(texture.raw()),
                0,
            );
            device.gl.draw_buffers(&[glow::NONE]);
            device.gl.read_buffer(glow::NONE);
            device.check_framebuffer("shadow");
            Self {
                device: device.clone(),
                texture,
                framebuffer: Cell::new(Some(framebuffer)),
                size,
            }
        }
    }
    pub fn begin(&self) {
        self.device
            .target(self.framebuffer.get(), self.size, self.size);
        self.device.clear(None, true);
    }
    pub fn destroy(&self) {
        if let Some(f) = self.framebuffer.take() {
            self.device.delete_framebuffer(f);
        }
        self.texture.destroy();
    }
}
impl Drop for DepthTarget {
    fn drop(&mut self) {
        self.destroy();
    }
}

pub struct ColorTarget {
    device: Device,
    pub width: u32,
    pub height: u32,
    pub resolved: Texture,
    framebuffer: Cell<Option<glow::Framebuffer>>,
    resolve: Cell<Option<glow::Framebuffer>>,
    color: Cell<Option<glow::Renderbuffer>>,
    depth: Cell<Option<glow::Renderbuffer>>,
}
impl ColorTarget {
    pub fn new(device: &Device, _label: &str, width: u32, height: u32, samples: u32) -> Self {
        unsafe {
            let resolved = Texture::new(device, glow::RGBA16F, width, height, 1);
            let framebuffer = device.gl.create_framebuffer().expect("HDR framebuffer");
            device.target(Some(framebuffer), width, height);
            let color = device.gl.create_renderbuffer().expect("MSAA color");
            device.gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
            device.gl.renderbuffer_storage_multisample(
                glow::RENDERBUFFER,
                samples as i32,
                glow::RGBA16F,
                width as i32,
                height as i32,
            );
            device.gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::RENDERBUFFER,
                Some(color),
            );
            let depth = device.gl.create_renderbuffer().expect("MSAA depth");
            device.gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
            device.gl.renderbuffer_storage_multisample(
                glow::RENDERBUFFER,
                samples as i32,
                glow::DEPTH_COMPONENT32F,
                width as i32,
                height as i32,
            );
            device.gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(depth),
            );
            device.check_framebuffer("MSAA HDR");
            let resolve = device.gl.create_framebuffer().expect("resolve framebuffer");
            device.target(Some(resolve), width, height);
            device.gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(resolved.raw()),
                0,
            );
            device.check_framebuffer("HDR resolve");
            Self {
                device: device.clone(),
                width,
                height,
                resolved,
                framebuffer: Cell::new(Some(framebuffer)),
                resolve: Cell::new(Some(resolve)),
                color: Cell::new(Some(color)),
                depth: Cell::new(Some(depth)),
            }
        }
    }
    pub fn begin(&self, clear: [f32; 4]) {
        self.device
            .target(self.framebuffer.get(), self.width, self.height);
        self.device.clear(Some(clear), true);
    }
    pub fn resolve(&self) {
        self.device
            .framebuffers(self.framebuffer.get(), self.resolve.get());
        unsafe {
            self.device.gl.blit_framebuffer(
                0,
                0,
                self.width as i32,
                self.height as i32,
                0,
                0,
                self.width as i32,
                self.height as i32,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
        }
    }
    pub fn bytes(&self, samples: u32) -> u64 {
        self.width as u64 * self.height as u64 * (12 * samples as u64 + 8)
    }
    pub fn destroy(&self) {
        for f in [&self.framebuffer, &self.resolve] {
            if let Some(f) = f.take() {
                self.device.delete_framebuffer(f);
            }
        }
        unsafe {
            for r in [&self.color, &self.depth] {
                if let Some(r) = r.take() {
                    self.device.gl.delete_renderbuffer(r);
                }
            }
        }
        self.resolved.destroy();
    }
}
impl Drop for ColorTarget {
    fn drop(&mut self) {
        self.destroy();
    }
}

pub struct OutputTarget {
    device: Device,
    texture: Texture,
    framebuffer: Cell<Option<glow::Framebuffer>>,
    pub width: u32,
    pub height: u32,
}
impl OutputTarget {
    pub fn new(device: &Device, width: u32, height: u32) -> Self {
        unsafe {
            let texture = Texture::new(device, glow::RGBA8, width, height, 1);
            let framebuffer = device.gl.create_framebuffer().expect("output framebuffer");
            device.target(Some(framebuffer), width, height);
            device.gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture.raw()),
                0,
            );
            device.check_framebuffer("output");
            Self {
                device: device.clone(),
                texture,
                framebuffer: Cell::new(Some(framebuffer)),
                width,
                height,
            }
        }
    }
    pub fn begin(&self) {
        self.device
            .target(self.framebuffer.get(), self.width, self.height);
        self.device.clear(Some([0., 0., 0., 1.]), false);
    }
    pub fn present(&self) {
        self.device.framebuffers(self.framebuffer.get(), None);
        unsafe {
            self.device.gl.blit_framebuffer(
                0,
                self.height as i32,
                self.width as i32,
                0,
                0,
                0,
                self.width as i32,
                self.height as i32,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
        }
    }
}
impl Drop for OutputTarget {
    fn drop(&mut self) {
        if let Some(f) = self.framebuffer.take() {
            self.device.delete_framebuffer(f);
        }
        self.texture.destroy();
    }
}
