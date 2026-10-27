//! The menus and HUD: egui laid out each frame from the window's input, then drawn
//! over the renderer's frame in the same window texture before it is shown.

use std::sync::Arc;

use egui_wgpu::ScreenDescriptor;
use sloppy_render::gpu::Renderer;
use sloppy_render::wgpu;
use winit::event::WindowEvent;
use winit::window::Window;

/// One frame of laid-out UI, ready to draw.
pub struct Painted {
    primitives: Vec<egui::ClippedPrimitive>,
    textures: egui::TexturesDelta,
    pixels_per_point: f32,
}

pub struct Overlay {
    pub context: egui::Context,
    input: egui_winit::State,
    painter: egui_wgpu::Renderer,
    window: Arc<Window>,
}

impl Overlay {
    pub fn new(window: Arc<Window>, renderer: &Renderer) -> Self {
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
        let (device, _) = renderer.device();
        let painter =
            egui_wgpu::Renderer::new(device, renderer.surface_format(), Default::default());
        Self {
            context,
            input,
            painter,
            window,
        }
    }

    /// Hand a window event to egui; true when egui used it (a click on a button, typing
    /// in a field), so the game should not see it.
    pub fn event(&mut self, event: &WindowEvent) -> bool {
        self.input.on_window_event(&self.window, event).consumed
    }

    /// Whether egui wants the pointer (it is over a window or a widget).
    pub fn wants_pointer(&self) -> bool {
        self.context.egui_wants_pointer_input() || self.context.is_pointer_over_egui()
    }

    /// Whether a text field has the keyboard.
    pub fn wants_keyboard(&self) -> bool {
        self.context.egui_wants_keyboard_input()
    }

    /// Lay out this frame's UI with `build`.
    pub fn run(&mut self, build: impl FnMut(&egui::Context)) -> Painted {
        let input = self.input.take_egui_input(&self.window);
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

    /// Draw `painted` over the frame's `view`. The window texture can be smaller
    /// than the window (the renderer caps its pixel ratio), so egui's points map to
    /// the texture's own pixels.
    pub fn paint(
        &mut self,
        painted: &Painted,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        target: (u32, u32),
    ) {
        let logical = self
            .window
            .inner_size()
            .to_logical::<f32>(self.window.scale_factor());
        let screen = ScreenDescriptor {
            size_in_pixels: [target.0, target.1],
            pixels_per_point: if logical.width > 0.0 {
                target.0 as f32 / logical.width
            } else {
                painted.pixels_per_point
            },
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
    pub fn free_textures(&mut self, painted: &Painted) {
        for id in &painted.textures.free {
            self.painter.free_texture(id);
        }
    }

    /// Show the renderer's frame with `painted` over it; between rounds, on a window
    /// cleared to the backdrop.
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
        self.free_textures(&painted);
    }
}
