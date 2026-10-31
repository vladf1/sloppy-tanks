//! The wgpu device (the browser's WebGPU, or natively Metal or Direct3D 12),
//! its canvas or window surface and the size-dependent frame attachments.
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
    /// The bloom levels' format ([`bloom_format`]).
    pub bloom_format: wgpu::TextureFormat,
    /// MSAA samples of the HDR targets: `SAMPLE_COUNT`, or 1 without multisampling.
    pub samples: u32,
    /// The adapter's largest 2D texture edge, a hint of the GPU's class.
    pub adapter_max_texture: u32,
    /// `TRANSIENT_ATTACHMENT` where the browser offers it, else empty.
    pub transient: wgpu::TextureUsages,
    pub error: ErrorSlot,
}

/// The canvas's surface and its configuration (format and drawing-buffer size).
pub struct Canvas {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
}

/// What the renderer draws into: the page's canvas.
#[cfg(target_arch = "wasm32")]
pub type RenderTarget = web_sys::HtmlCanvasElement;

/// What the renderer draws into natively: a window and its size in device pixels.
#[cfg(not(target_arch = "wasm32"))]
pub struct RenderTarget {
    pub window: wgpu::SurfaceTarget<'static>,
    pub width: u32,
    pub height: u32,
}

/// The device and the canvas (natively the window's surface) configured for it: what a
/// renderer starts on. The page opens it before the engine half of its binary has
/// arrived and draws Battle Setup on it; the renderer then takes it over
/// (`Renderer::on`), so the page asks for one device and the canvas keeps the menu's
/// last frame until the engine's first.
pub struct GpuCanvas {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    canvas: Canvas,
    /// The adapter's features and largest 2D texture edge.
    features: wgpu::Features,
    max_texture: u32,
    error: ErrorSlot,
    /// The page's canvas element, which the surface consumed.
    #[cfg(target_arch = "wasm32")]
    element: web_sys::HtmlCanvasElement,
    /// The window took the last frame; a covered window takes none.
    shown: bool,
}

impl GpuCanvas {
    /// Ask for the adapter and device and configure `target` for them. Fails when the
    /// build's graphics API is unavailable.
    pub async fn open(target: RenderTarget) -> Result<Self, String> {
        let api = GRAPHICS_API;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: BACKENDS,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        #[cfg(target_arch = "wasm32")]
        let (width, height) = (target.width().max(1), target.height().max(1));
        #[cfg(not(target_arch = "wasm32"))]
        let (width, height) = (target.width.max(1), target.height.max(1));
        // A window's surface exists first natively, so the adapter is one that can
        // present to it.
        #[cfg(not(target_arch = "wasm32"))]
        let surface = instance
            .create_surface(target.window)
            .map_err(|error| format!("{api} window unavailable: {error}"))?;
        // WebGPU claims the canvas only once it has a device: a page whose WebGPU
        // device fails falls back to WebGL on the same canvas (`src/engine.ts`), and a
        // canvas with a WebGPU context has no WebGL one.
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                #[cfg(not(target_arch = "wasm32"))]
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("{api} adapter unavailable: {error}"))?;
        // WebGPU's defaults, but no more inter-stage variables than the adapter has:
        // the iOS simulator's Metal offers 15 of the 16, and the shaders fit in 15.
        // Browsers, Macs and iPads offer at least 16, so they get the defaults.
        let defaults = wgpu::Limits::default();
        let required_limits = wgpu::Limits {
            max_inter_stage_shader_variables: defaults
                .max_inter_stage_shader_variables
                .min(adapter.limits().max_inter_stage_shader_variables),
            ..defaults
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Sloppy Tanks renderer"),
                required_features: adapter.features() & wgpu::Features::RG11B10UFLOAT_RENDERABLE,
                required_limits,
                ..Default::default()
            })
            .await
            .map_err(|error| format!("{api} device unavailable: {error}"))?;
        #[cfg(target_arch = "wasm32")]
        let element = target.clone();
        #[cfg(target_arch = "wasm32")]
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(target))
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
        // lists both), so this fails only for a native window no adapter presents to.
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| format!("{api} cannot present to this window"))?;
        // The output pass encodes sRGB itself (Three's sRGBTransferOETF), so the
        // canvas keeps its preferred non-sRGB format.
        config.format = config.format.remove_srgb_suffix();
        config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        surface.configure(&device, &config);
        Ok(Self {
            device,
            queue,
            canvas: Canvas { surface, config },
            features: adapter.features(),
            max_texture: adapter.limits().max_texture_dimension_2d,
            error,
            #[cfg(target_arch = "wasm32")]
            element,
            shown: true,
        })
    }

    /// The canvas's texture format, which an overlay draws in.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.canvas.config.format
    }

    /// The page's canvas element.
    #[cfg(target_arch = "wasm32")]
    pub fn element(&self) -> &web_sys::HtmlCanvasElement {
        &self.element
    }

    /// The drawing buffer's size in device pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.canvas.config.width, self.canvas.config.height)
    }

    /// Size the drawing buffer to `size` device pixels, as the renderer's frame does.
    pub fn resize(&mut self, (width, height): (u32, u32)) {
        let fit = |side: u32| side.clamp(1, self.max_texture);
        let size = (fit(width), fit(height));
        let config = &mut self.canvas.config;
        if (config.width, config.height) != size {
            (config.width, config.height) = size;
            self.canvas.surface.configure(&self.device, config);
        }
    }

    /// Whether the window took the last frame; a covered window takes none.
    pub fn frame_shown(&self) -> bool {
        self.shown
    }

    /// The first error the device reported, if any.
    pub fn error(&self) -> Option<String> {
        self.error.get()
    }

    /// Show a frame cleared to `clear` with `overlay` (the menus) drawn over it with the
    /// device, the queue and the frame's view, as the renderer shows one before any
    /// scene (`Frame::present`).
    pub fn present(
        &mut self,
        clear: wgpu::Color,
        overlay: impl FnOnce(&wgpu::Device, &wgpu::Queue, &wgpu::TextureView),
    ) {
        let output = match self.canvas.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.canvas
                    .surface
                    .configure(&self.device, &self.canvas.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                self.shown = false;
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout => return,
            error => {
                self.error.set(format!("Window unavailable: {error:?}"));
                return;
            }
        };
        let view = output.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blank window"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        self.queue.submit([encoder.finish()]);
        overlay(&self.device, &self.queue, &view);
        self.queue.present(output);
        self.shown = true;
    }
}

impl Gpu {
    /// The renderer's device on `opened`, multisampled when `msaa` (fixed for its life).
    pub fn on(opened: GpuCanvas, msaa: bool) -> Result<(Self, Canvas), String> {
        let GpuCanvas {
            device,
            queue,
            canvas,
            features,
            max_texture,
            error,
            ..
        } = opened;
        let samples = if msaa { SAMPLE_COUNT } else { 1 };
        let gpu = Self {
            layouts: Rc::new(Layouts::new(&device)),
            canvas_format: canvas.config.format,
            bloom_format: bloom_format(features),
            samples,
            adapter_max_texture: max_texture,
            transient: transient_usage(),
            device,
            queue,
            error,
        };
        Ok((gpu, canvas))
    }
}

impl Gpu {
    /// The largest texture edge the adapter offers.
    pub fn max_texture_size(&self) -> u32 {
        self.adapter_max_texture
    }
}

/// The graphics APIs wgpu may draw with: the browser's WebGPU, or natively Metal or
/// Direct3D 12. Not Vulkan: its swapchain must match the window's pixels, while the
/// canvas caps its pixel ratio (a Windows display scaled past 150% would refuse it),
/// and wgpu could pick it over Direct3D 12, which every Windows 10 machine has.
#[cfg(target_arch = "wasm32")]
const BACKENDS: wgpu::Backends = wgpu::Backends::BROWSER_WEBGPU;
#[cfg(not(target_arch = "wasm32"))]
const BACKENDS: wgpu::Backends = wgpu::Backends::METAL.union(wgpu::Backends::DX12);

/// `GPUTextureUsage.TRANSIENT_ATTACHMENT` where the browser has it (Safari 26 and
/// Chrome do): an attachment cleared and discarded within its pass then lives only
/// in tile memory and holds no memory at all on tiling GPUs (Apple's, phones').
/// wgpu passes usage bits through to the browser, which rejects a bit it does not
/// know, so older browsers get plain attachments.
#[cfg(target_arch = "wasm32")]
fn transient_usage() -> wgpu::TextureUsages {
    let transient = wgpu::TextureUsages::TRANSIENT_ATTACHMENT;
    let offered = js_sys::Reflect::get(&js_sys::global(), &"GPUTextureUsage".into())
        .and_then(|usages| js_sys::Reflect::get(&usages, &"TRANSIENT_ATTACHMENT".into()))
        .ok()
        .and_then(|bit| bit.as_f64());
    if offered == Some(f64::from(transient.bits())) {
        transient
    } else {
        wgpu::TextureUsages::empty()
    }
}

/// Native wgpu always has the usage; where the GPU cannot keep an attachment in tile
/// memory (Direct3D 12) it is an ordinary attachment.
#[cfg(not(target_arch = "wasm32"))]
fn transient_usage() -> wgpu::TextureUsages {
    wgpu::TextureUsages::TRANSIENT_ATTACHMENT
}

/// Bytes a texture holds in memory: none for a transient attachment.
pub(super) fn texture_bytes(texture: &wgpu::Texture) -> u64 {
    if texture
        .usage()
        .contains(wgpu::TextureUsages::TRANSIENT_ATTACHMENT)
    {
        return 0;
    }
    let texel = texture.format().block_copy_size(None).unwrap_or(4);
    u64::from(texture.width())
        * u64::from(texture.height())
        * u64::from(texture.sample_count())
        * u64::from(texel)
}

/// The scene's HDR color: alpha to coverage (foliage) needs the target's alpha, so not
/// `rg11b10ufloat`.
pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The bloom levels' format: `rg11b10ufloat` where the adapter can render to it
/// (Safari 26, Chrome), half the bytes of `rgba16float`; a soft glow needs neither
/// alpha nor more precision.
fn bloom_format(features: wgpu::Features) -> wgpu::TextureFormat {
    if features.contains(wgpu::Features::RG11B10UFLOAT_RENDERABLE) {
        wgpu::TextureFormat::Rg11b10Ufloat
    } else {
        HDR_FORMAT
    }
}
/// The ambient occlusion target.
pub const OCCLUSION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// The sun's shadow maps. Their orthographic depth spans `SHADOW_DEPTH` (about 220 m)
/// linearly, so 16 bits resolve 3.4 mm, a thirteenth of the shadow bias; half the bytes
/// of a 32-bit float halve the maps' memory and the copy, load and store of the cached
/// fixed scenery every frame.
pub const SHADOW_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth16Unorm;

/// A multisampled HDR color + depth target and its single-sample resolve, or without
/// multisampling a color target passes draw into directly. Used for the main view
/// (the scene's size) and the water reflection (fixed size). The multisampled color
/// is resolved and the depth discarded within the pass (ambient occlusion draws a
/// depth of its own), so both are transient where the browser allows. Dropping it
/// destroys its textures.
pub struct ColorTarget {
    /// The multisampled color; absent without multisampling.
    pub color: Option<wgpu::Texture>,
    /// Where passes draw: the multisampled color, or the resolved texture itself.
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
    pub fn new(gpu: &Gpu, label: &str, width: u32, height: u32) -> Self {
        let texture = |format, samples, usage| {
            texture_2d(&gpu.device, label, width, height, format, samples, usage)
        };
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let samples = gpu.samples;
        let color = (samples > 1).then(|| texture(HDR_FORMAT, samples, attachment | gpu.transient));
        let depth = texture(DEPTH_FORMAT, samples, attachment | gpu.transient);
        let resolved = texture(
            HDR_FORMAT,
            1,
            attachment | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        Self {
            color_view: color
                .as_ref()
                .unwrap_or(&resolved)
                .create_view(&Default::default()),
            depth_view: depth.create_view(&Default::default()),
            resolved_view: resolved.create_view(&Default::default()),
            color,
            depth,
            resolved,
        }
    }

    /// Bytes its textures hold in memory.
    pub fn bytes(&self) -> u64 {
        [self.color.as_ref(), Some(&self.depth), Some(&self.resolved)]
            .into_iter()
            .flatten()
            .map(texture_bytes)
            .sum()
    }

    /// The texture a multisampled pass resolves into; none without multisampling.
    pub fn resolve_target(&self) -> Option<&wgpu::TextureView> {
        self.color.as_ref().map(|_| &self.resolved_view)
    }
}

impl Drop for ColorTarget {
    fn drop(&mut self) {
        if let Some(color) = &self.color {
            color.destroy();
        }
        self.depth.destroy();
        self.resolved.destroy();
    }
}
