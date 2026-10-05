//! Browser WebGPU device, canvas surface and the
//! size-dependent frame attachments.
//! GPU validation errors and device loss are recorded and surfaced by the next
//! `render` call; the page stops and shows them instead of drawing on.

use std::sync::{Arc, Mutex};

/// The first GPU failure, shared with wgpu's callbacks.
#[derive(Clone, Default)]
pub struct ErrorSlot(Arc<Mutex<Option<String>>>);

impl ErrorSlot {
    pub fn set(&self, message: String) {
        let mut slot = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if slot.is_none() {
            web_sys::console::error_1(&message.clone().into());
            *slot = Some(message);
        }
    }

    pub fn get(&self) -> Option<String> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

/// The browser API this build draws with; startup errors name it, and the page
/// tells an unavailable one from other failures by the prefix (`src/engine.ts`).
use super::GRAPHICS_API;

pub struct Context {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub error: ErrorSlot,
}

impl Context {
    pub async fn new(canvas: web_sys::HtmlCanvasElement) -> Result<Self, String> {
        let api = GRAPHICS_API;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let create_surface = |canvas| {
            instance
                .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
                .map_err(|error| format!("{api} canvas unavailable: {error}"))
        };
        // Claim the canvas only after obtaining a device, so a failed device
        // leaves it available for the independently downloaded WebGL engine.
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
        let surface = create_surface(canvas)?;
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
        let mut config = match surface.get_default_config(&adapter, width, height) {
            Some(config) => config,
            // Every WebGPU canvas takes rgba8unorm, so WebGPU never fails here, once
            // it has claimed the canvas; the page could not fall back on it any more.
            None => wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: wgpu::TextureFormat::Rgba8Unorm,
                color_space: Default::default(),
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                view_formats: vec![],
            },
        };
        // The output pass encodes sRGB itself (Three's sRGBTransferOETF), so the
        // canvas keeps its preferred non-sRGB format.
        config.format = config.format.remove_srgb_suffix();
        config.view_formats = vec![];
        config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        surface.configure(&device, &config);
        Ok(Self {
            surface,
            device,
            queue,
            config,
            error,
        })
    }
}

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A multisampled HDR color + depth target and its single-sample resolve.
/// Used for the main view (canvas-sized) and the water reflection (fixed size).
pub struct ColorTarget {
    pub width: u32,
    pub height: u32,
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
            width,
            height,
            color_view: color.create_view(&Default::default()),
            depth_view: depth.create_view(&Default::default()),
            resolved_view: resolved.create_view(&Default::default()),
            color,
            depth,
            resolved,
        }
    }

    /// Bytes held (MSAA color and depth, plus the resolve).
    pub fn bytes(&self, samples: u32) -> u64 {
        let pixels = self.width as u64 * self.height as u64;
        pixels * (8 * samples as u64 + 4 * samples as u64 + 8)
    }

    pub fn destroy(&self) {
        self.color.destroy();
        self.depth.destroy();
        self.resolved.destroy();
    }
}
