//! The menus and HUD: egui laid out each frame from the window's input, then drawn
//! over the renderer's frame in the same window texture before it is shown: with
//! egui-wgpu on the renderer's wgpu device (natively, and on the browser's WebGPU), or
//! with egui_glow on its WebGL2 context (the page's `webgl` build).

use std::sync::Arc;

#[cfg(not(feature = "webgl"))]
use egui_wgpu::ScreenDescriptor;
use sloppy_render::gpu::Renderer;
#[cfg(not(feature = "webgl"))]
use sloppy_render::wgpu;
use winit::event::WindowEvent;
use winit::window::Window;

#[cfg(not(feature = "webgl"))]
type Painter = egui_wgpu::Renderer;
#[cfg(feature = "webgl")]
type Painter = egui_glow::Painter;

/// The painter on the renderer's device.
#[cfg(not(feature = "webgl"))]
fn painter(renderer: &Renderer) -> Result<Painter, String> {
    let (device, _) = renderer.device();
    Ok(egui_wgpu::Renderer::new(
        device,
        renderer.surface_format(),
        Default::default(),
    ))
}

/// The painter on the renderer's WebGL2 context, through a glow handle of its own.
#[cfg(feature = "webgl")]
#[expect(
    clippy::arc_with_non_send_sync,
    reason = "egui_glow takes its context in an Arc; a page has one thread"
)]
fn painter(renderer: &Renderer) -> Result<Painter, String> {
    let gl = egui_glow::glow::Context::from_webgl2_context(renderer.webgl_context());
    egui_glow::Painter::new(Arc::new(gl), "", None, false).map_err(|error| error.to_string())
}

/// One frame of laid-out UI, ready to draw.
pub struct Painted {
    primitives: Vec<egui::ClippedPrimitive>,
    textures: egui::TexturesDelta,
    pixels_per_point: f32,
}

pub struct Overlay {
    pub context: egui::Context,
    input: egui_winit::State,
    painter: Painter,
    window: Arc<Window>,
}

impl Overlay {
    pub fn new(window: Arc<Window>, renderer: &Renderer) -> Result<Self, String> {
        let context = egui::Context::default();
        crate::ui::theme::install(&context);
        let input = egui_winit::State::new(
            context.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let painter = painter(renderer)?;
        Ok(Self {
            context,
            input,
            painter,
            window,
        })
    }

    /// Hand a window event to egui; true when egui used it (a click on a button, typing
    /// in a field), so the game should not see it.
    ///
    /// While the cursor is locked for first person the hidden pointer stays parked
    /// wherever it was, so `pointer` is false and egui sees no pointer events: a
    /// click there is a shot, as on the page under pointer lock. While the battle has
    /// the keyboard `keys` is false: a HUD button that took focus would otherwise
    /// swallow the drive keys, Space and Esc.
    pub fn event(&mut self, event: &WindowEvent, pointer: bool, keys: bool) -> bool {
        let skipped = match event {
            WindowEvent::CursorMoved { .. }
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. } => !pointer,
            WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_) => !keys,
            _ => false,
        };
        !skipped && self.input.on_window_event(&self.window, event).consumed
    }

    /// Whether egui wants the pointer (it is over a window or a widget).
    pub fn wants_pointer(&self) -> bool {
        self.context.egui_wants_pointer_input() || self.context.is_pointer_over_egui()
    }

    /// Whether a text field has the keyboard (typing a name or a room code).
    pub fn typing(&self) -> bool {
        self.context.text_edit_focused()
    }

    /// Lay out this frame's UI with `build`, on the window's size in points as the
    /// engines take it.
    pub fn run(&mut self, build: impl FnMut(&egui::Context)) -> Painted {
        let mut input = self.input.take_egui_input(&self.window);
        let (width, height, _) = crate::app::css_size(&self.window);
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width as f32, height as f32),
        ));
        let output = self.context.run_pass(input, build);
        self.input
            .handle_platform_output(&self.window, output.platform_output);
        Painted {
            primitives: self
                .context
                .tessellate(output.shapes, output.pixels_per_point),
            textures: output.textures_delta,
            pixels_per_point: output.pixels_per_point,
        }
    }

    /// The frame's pixels per point of egui's: the window texture can be smaller than
    /// the window (the renderer caps its pixel ratio), so egui's points map to the
    /// texture's own pixels.
    fn pixels_per_point(&self, target_width: u32, painted: &Painted) -> f32 {
        let (width, _, _) = crate::app::css_size(&self.window);
        if width > 0.0 {
            target_width as f32 / width as f32
        } else {
            painted.pixels_per_point
        }
    }

    /// Draw `painted` over the frame's `view`.
    #[cfg(not(feature = "webgl"))]
    pub fn paint(
        &mut self,
        painted: &Painted,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        target: (u32, u32),
    ) {
        let screen = ScreenDescriptor {
            size_in_pixels: [target.0, target.1],
            pixels_per_point: self.pixels_per_point(target.0, painted),
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("overlay"),
        });
        let mut buffers =
            self.painter
                .update_buffers(device, queue, &mut encoder, &painted.primitives, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            self.painter
                .render(&mut pass.forget_lifetime(), &painted.primitives, &screen);
        }
        buffers.push(encoder.finish());
        queue.submit(buffers);
    }

    /// Apply the frame's texture changes, whether or not the window shows the frame:
    /// later changes only patch what these create.
    #[cfg(not(feature = "webgl"))]
    pub fn update_textures(
        &mut self,
        painted: &mut Painted,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        for (id, deltas) in &painted.textures.set {
            for delta in deltas {
                self.painter.update_texture(device, queue, *id, delta);
            }
        }
        painted.textures.set.clear();
    }

    /// Free the textures the frame let go of, once it has drawn.
    #[cfg(not(feature = "webgl"))]
    pub fn free_textures(&mut self, painted: &mut Painted) {
        for id in &painted.textures.free {
            self.painter.free_texture(id);
        }
        painted.textures.free.clear();
    }

    /// Show the renderer's frame with `painted` over it; between rounds, on a window
    /// cleared to the backdrop.
    #[cfg(not(feature = "webgl"))]
    pub fn present(&mut self, renderer: &mut Renderer, mut painted: Painted) {
        let target = renderer.size();
        let (device, queue) = renderer.device();
        self.update_textures(&mut painted, device, queue);
        let [red, green, blue, _] = crate::ui::theme::BACKDROP.to_normalized_gamma_f32();
        let clear = wgpu::Color {
            r: f64::from(red),
            g: f64::from(green),
            b: f64::from(blue),
            a: 1.0,
        };
        renderer.present(clear, |device, queue, view| {
            self.paint(&painted, device, queue, view, target);
        });
        self.free_textures(&mut painted);
    }

    /// Show the renderer's frame with `painted` over it, on WebGL; between rounds, on a
    /// canvas cleared to the backdrop.
    #[cfg(feature = "webgl")]
    pub fn present(&mut self, renderer: &mut Renderer, mut painted: Painted) {
        let [red, green, blue, _] = crate::ui::theme::BACKDROP.to_normalized_gamma_f32();
        let pixels_per_point = self.pixels_per_point(renderer.size().0, &painted);
        let painter = &mut self.painter;
        renderer.present([red, green, blue], |(width, height)| {
            painter.paint_and_update_textures(
                [width, height],
                pixels_per_point,
                &painted.primitives,
                &mut painted.textures,
            );
        });
    }
}

/// egui_glow's GL objects go with the overlay, before the next engine's.
#[cfg(feature = "webgl")]
impl Drop for Overlay {
    fn drop(&mut self) {
        self.painter.destroy();
    }
}
