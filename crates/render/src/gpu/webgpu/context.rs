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
                compatible_surface: None,
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|error| format!("{api} adapter unavailable: {error}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Sloppy Tanks renderer"),
                required_limits: wgpu::Limits::default(),
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
        // Every WebGPU canvas takes rgba8unorm, so WebGPU never fails here, once it has
        // claimed the canvas; the page could not fall back on it any more.
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .unwrap_or_else(|| wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: wgpu::TextureFormat::Rgba8Unorm,
                color_space: Default::default(),
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                view_formats: vec![],
            });
        // The output pass encodes sRGB itself (Three's sRGBTransferOETF), so the
        // canvas keeps its preferred non-sRGB format.
        config.format = config.format.remove_srgb_suffix();
        config.view_formats = vec![];
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

fn attachment(
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
    pub fn new(device: &wgpu::Device, label: &str, width: u32, height: u32, samples: u32) -> Self {
        let color = attachment(
            device,
            label,
            width,
            height,
            HDR_FORMAT,
            samples,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let depth = attachment(
            device,
            label,
            width,
            height,
            DEPTH_FORMAT,
            samples,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let resolved = attachment(
            device,
            label,
            width,
            height,
            HDR_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
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
