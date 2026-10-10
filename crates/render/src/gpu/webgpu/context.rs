//! Browser WebGPU device, canvas surface and the size-dependent frame attachments.
//! GPU validation errors and device loss go to the renderer's [`ErrorSlot`].

use std::rc::Rc;
use std::sync::Arc;

use super::resources::Layouts;
use crate::gpu::SAMPLE_COUNT;
use crate::gpu::context::{ErrorSlot, GRAPHICS_API};

/// The device and queue, with the bind group layouts every pipeline shares. Cheap to
/// clone.
#[derive(Clone)]
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub layouts: Rc<Layouts>,
    /// The canvas's texture format, the output pipeline's target.
    pub canvas_format: wgpu::TextureFormat,
    /// MSAA samples of the HDR targets: always `SAMPLE_COUNT` here.
    pub samples: u32,
    pub error: ErrorSlot,
}

/// The canvas's surface and its configuration (format and drawing-buffer size).
pub struct Canvas {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
}

impl Gpu {
    pub async fn new(canvas: web_sys::HtmlCanvasElement) -> Result<(Self, Canvas), String> {
        let api = GRAPHICS_API;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        // WebGPU claims the canvas only once it has a device: a page whose WebGPU
        // device fails falls back to WebGL on the same canvas (`src/engine.ts`), and a
        // canvas with a WebGPU context has no WebGL one.
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                ..Default::default()
            })
            .await
            .map_err(|error| format!("{api} adapter unavailable: {error}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Sloppy Tanks renderer"),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("{api} device unavailable: {error}"))?;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|error| format!("{api} canvas unavailable: {error}"))?;
        let error = ErrorSlot::default();
        let lost = error.clone();
        device.set_device_lost_callback(move |reason, message| {
            if reason != wgpu::DeviceLostReason::Destroyed {
                lost.set(format!("GPU device lost: {message}. Reload to restart."));
            }
        });
        let validation = error.clone();
        device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
            validation.set(format!("{api} error: {error}"));
        }));
        // Every WebGPU canvas offers rgba8unorm and bgra8unorm (wgpu's browser backend
        // lists both), so this never fails once WebGPU has claimed the canvas, where
        // the page could no longer fall back.
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .expect("a WebGPU canvas offers rgba8unorm and bgra8unorm");
        // The output pass encodes sRGB itself (Three's sRGBTransferOETF), so the
        // canvas keeps its preferred non-sRGB format.
        config.format = config.format.remove_srgb_suffix();
        config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        surface.configure(&device, &config);
        let gpu = Self {
            layouts: Rc::new(Layouts::new(&device)),
            canvas_format: config.format,
            samples: SAMPLE_COUNT,
            device,
            queue,
            error,
        };
        Ok((gpu, Canvas { surface, config }))
    }
}

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A multisampled HDR color + depth target and its single-sample resolve.
/// Used for the main view (canvas-sized) and the water reflection (fixed size).
/// Dropping it destroys its textures.
pub struct ColorTarget {
    pub color: wgpu::Texture,
    pub color_view: wgpu::TextureView,
    pub depth: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    pub resolved: wgpu::Texture,
    pub resolved_view: wgpu::TextureView,
}

/// A single-level 2D texture.
pub(super) fn texture_2d(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    samples: u32,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

impl ColorTarget {
    pub fn new(device: &wgpu::Device, label: &str, width: u32, height: u32) -> Self {
        let texture = |format, samples, usage| {
            texture_2d(device, label, width, height, format, samples, usage)
        };
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let color = texture(HDR_FORMAT, SAMPLE_COUNT, attachment);
        let depth = texture(DEPTH_FORMAT, SAMPLE_COUNT, attachment);
        let resolved = texture(
            HDR_FORMAT,
            1,
            attachment | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        Self {
            color_view: color.create_view(&Default::default()),
            depth_view: depth.create_view(&Default::default()),
            resolved_view: resolved.create_view(&Default::default()),
            color,
            depth,
            resolved,
        }
    }
}

impl Drop for ColorTarget {
    fn drop(&mut self) {
        self.color.destroy();
        self.depth.destroy();
        self.resolved.destroy();
    }
}
