//! The WebGL2 context and the GL state it last set.
//!
//! Every GL call that changes binding or fixed-function state goes through [`Gl`],
//! which compares it with what it set last and skips the call when nothing changes:
//! each call is a crossing into JavaScript and a command the browser validates and
//! forwards to its GPU process. Uploads never disturb what draws bound: buffers are
//! written through `COPY_WRITE_BUFFER` and textures set up on a unit of their own
//! ([`unit::UPLOAD`]), so a vertex array's element buffer and the units draws
//! sample stay as the cache believes. Deleting an object needs no cache update:
//! glow names objects with generational keys, so a deleted name never compares equal
//! to a live one.

use std::cell::RefCell;
use std::rc::Rc;

use glow::HasContext;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::gpu::context::{ErrorSlot, GRAPHICS_API};
pub use crate::shader::glsl::{block, unit};

/// Faces culled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cull {
    None,
    Back,
    Front,
}

/// Color blending (`shader::BlendMode` as GL factors).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Replace,
    /// srcAlpha, oneMinusSrcAlpha; alpha one, oneMinusSrcAlpha.
    Normal,
    /// srcAlpha, one; alpha one, one.
    Additive,
}

/// The fixed-function state a pipeline draws with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Raster {
    pub cull: Cull,
    /// `None` disables the depth test (targets without depth); WebGPU's `Always`
    /// compare stays a test that always passes, so depth writes still happen.
    pub depth_func: Option<u32>,
    pub depth_write: bool,
    pub blend: Blend,
    /// `polygonOffset(factor, units)`; zero is off.
    pub polygon_offset: (f32, f32),
    pub alpha_to_coverage: bool,
}

impl Raster {
    /// Full-screen passes: no depth, culling or blending.
    pub const PLAIN: Raster = Raster {
        cull: Cull::None,
        depth_func: None,
        depth_write: false,
        blend: Blend::Replace,
        polygon_offset: (0.0, 0.0),
        alpha_to_coverage: false,
    };
}

/// What the context has bound and enabled, as last set through [`Gl`].
struct State {
    program: Option<glow::Program>,
    vertex_array: Option<glow::VertexArray>,
    array_buffer: Option<glow::Buffer>,
    copy_write_buffer: Option<glow::Buffer>,
    uniform_blocks: [Option<glow::Buffer>; block::COUNT],
    active_unit: u32,
    textures: [Option<glow::Texture>; unit::COUNT],
    samplers: [Option<glow::Sampler>; unit::COUNT],
    draw_framebuffer: Option<glow::Framebuffer>,
    read_framebuffer: Option<glow::Framebuffer>,
    viewport: [i32; 4],
    clear_color: [f32; 4],
    flip_y: bool,
    raster: Raster,
}

/// The context, its capabilities and the state cache. Shared by every GL object
/// that deletes itself on drop.
pub struct Gl {
    pub gl: glow::Context,
    /// The same context for what glow does not offer (context loss).
    raw: web_sys::WebGl2RenderingContext,
    pub canvas: web_sys::HtmlCanvasElement,
    state: RefCell<State>,
    pub error: ErrorSlot,
    /// The largest texture and renderbuffer edge.
    pub max_size: u32,
    pub anisotropy: bool,
    /// `KHR_parallel_shader_compile`: programs link in the background.
    pub parallel_compile: bool,
    /// The context-loss listener, removed on drop.
    lost: Closure<dyn FnMut(web_sys::Event)>,
}

/// The renderer's handle on [`Gl`]. Cheap to clone.
#[derive(Clone)]
pub struct Gpu(Rc<Gl>);

impl std::ops::Deref for Gpu {
    type Target = Gl;
    fn deref(&self) -> &Gl {
        &self.0
    }
}

/// The canvas's drawing-buffer size; the default framebuffer is the canvas itself.
pub struct Canvas {
    pub width: u32,
    pub height: u32,
}

/// What `getContext("webgl2")` asks for. The default framebuffer only ever receives
/// the output pass's full-screen triangle: no depth or stencil, opaque like the
/// WebGPU canvas, and no multisampling (the scene resolves its own).
fn context_options() -> js_sys::Object {
    let options = js_sys::Object::new();
    for (name, value) in [
        ("antialias", false),
        ("depth", false),
        ("stencil", false),
        ("alpha", false),
    ] {
        js_sys::Reflect::set(&options, &name.into(), &value.into()).expect("plain object property");
    }
    options
}

impl Gpu {
    pub async fn new(canvas: web_sys::HtmlCanvasElement) -> Result<(Self, Canvas), String> {
        let api = GRAPHICS_API;
        let unavailable = |reason: &str| format!("{api} canvas unavailable: {reason}");
        let raw: web_sys::WebGl2RenderingContext = canvas
            .get_context_with_context_options("webgl2", &context_options())
            .map_err(|_| unavailable("WebGL2 context creation failed"))?
            .ok_or_else(|| unavailable("WebGL2 is not supported"))?
            .dyn_into()
            .map_err(|_| unavailable("not a WebGL2 context"))?;
        let gl = glow::Context::from_webgl2_context(raw.clone());
        let extensions = gl.supported_extensions();
        // The HDR targets are RGBA16F renderbuffers and textures drawn into.
        if !extensions.contains("EXT_color_buffer_float") {
            return Err(unavailable("EXT_color_buffer_float is missing"));
        }
        let anisotropy = extensions.contains("EXT_texture_filter_anisotropic");
        let parallel_compile = extensions.contains("KHR_parallel_shader_compile");
        let error = ErrorSlot::default();
        let lost = error.clone();
        let on_lost = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            lost.set("GPU device lost: the WebGL context was lost. Reload to restart.".into());
        });
        canvas
            .add_event_listener_with_callback("webglcontextlost", on_lost.as_ref().unchecked_ref())
            .map_err(|_| unavailable("cannot watch for context loss"))?;
        let (max_texture, max_renderbuffer) = unsafe {
            (
                gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE),
                gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE),
            )
        };
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let state = State {
            program: None,
            vertex_array: None,
            array_buffer: None,
            copy_write_buffer: None,
            uniform_blocks: [None; block::COUNT],
            active_unit: 0,
            textures: [None; unit::COUNT],
            samplers: [None; unit::COUNT],
            draw_framebuffer: None,
            read_framebuffer: None,
            viewport: [0, 0, width as i32, height as i32],
            clear_color: [0.0; 4],
            flip_y: false,
            raster: Raster::PLAIN,
        };
        apply_initial_state(&gl, &state);
        let gl = Gl {
            gl,
            raw,
            canvas,
            state: RefCell::new(state),
            error,
            max_size: max_texture.min(max_renderbuffer).max(1) as u32,
            anisotropy,
            parallel_compile,
            lost: on_lost,
        };
        Ok((Self(Rc::new(gl)), Canvas { width, height }))
    }
}

/// Set the context to the cache's first record, whatever earlier users of the canvas
/// left bound or enabled, and the state the backend never changes afterwards.
fn apply_initial_state(gl: &glow::Context, state: &State) {
    unsafe {
        gl.use_program(None);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.bind_buffer(glow::COPY_WRITE_BUFFER, None);
        for point in 0..block::COUNT as u32 {
            gl.bind_buffer_base(glow::UNIFORM_BUFFER, point, None);
        }
        for unit in (0..unit::COUNT as u32).rev() {
            gl.active_texture(glow::TEXTURE0 + unit);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_sampler(unit, None);
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        let [x, y, width, height] = state.viewport;
        gl.viewport(x, y, width, height);
        gl.clear_color(0.0, 0.0, 0.0, 0.0);
        gl.pixel_store_bool(web_sys::WebGl2RenderingContext::UNPACK_FLIP_Y_WEBGL, false);
        for capability in [
            glow::CULL_FACE,
            glow::DEPTH_TEST,
            glow::BLEND,
            glow::POLYGON_OFFSET_FILL,
            glow::SAMPLE_ALPHA_TO_COVERAGE,
            glow::SCISSOR_TEST,
            glow::STENCIL_TEST,
            // WebGPU does not dither.
            glow::DITHER,
        ] {
            gl.disable(capability);
        }
        gl.depth_mask(false);
        gl.color_mask(true, true, true, true);
        gl.blend_equation(glow::FUNC_ADD);
        // Shaders flip clip-space Y (`ADJUST_COORDINATE_SPACE`), which reverses the
        // winding WebGPU's counter-clockwise front faces have on screen.
        gl.front_face(glow::CW);
        // WebGPU does not convert uploads' color spaces either.
        gl.pixel_store_i32(
            web_sys::WebGl2RenderingContext::UNPACK_COLORSPACE_CONVERSION_WEBGL,
            glow::NONE as i32,
        );
    }
}

impl Drop for Gl {
    fn drop(&mut self) {
        let _ = self.canvas.remove_event_listener_with_callback(
            "webglcontextlost",
            self.lost.as_ref().unchecked_ref(),
        );
    }
}

impl Gl {
    /// The first GPU error or context loss, if any.
    pub fn error(&self) -> Option<String> {
        self.error.get()
    }

    /// A new GL object. Creation fails only once the context is lost; then the
    /// renderer stops: the loss goes to the error slot and the call panics, which
    /// the page reports like any engine failure.
    pub fn created<T>(&self, object: Result<T, String>, what: &str) -> T {
        object.unwrap_or_else(|error| {
            let message = format!(
                "GPU device lost: cannot create a WebGL {what} ({error}). Reload to restart."
            );
            self.error.set(message.clone());
            panic!("{message}");
        })
    }

    /// Record a GL error the context reports, if any; `getError` waits for the GPU
    /// process, so callers check only now and then.
    pub fn check_error(&self, during: &str) {
        if self.raw.is_context_lost() {
            return;
        }
        let code = unsafe { self.gl.get_error() };
        if code != glow::NO_ERROR {
            self.error
                .set(format!("{GRAPHICS_API} error 0x{code:04x} while {during}"));
        }
    }

    pub fn use_program(&self, program: glow::Program) {
        let mut state = self.state.borrow_mut();
        if state.program != Some(program) {
            state.program = Some(program);
            unsafe { self.gl.use_program(Some(program)) };
        }
    }

    pub fn bind_vertex_array(&self, vertex_array: Option<glow::VertexArray>) {
        let mut state = self.state.borrow_mut();
        if state.vertex_array != vertex_array {
            state.vertex_array = vertex_array;
            unsafe { self.gl.bind_vertex_array(vertex_array) };
        }
    }

    /// `ARRAY_BUFFER`, which `vertexAttribPointer` reads from.
    pub fn bind_array_buffer(&self, buffer: glow::Buffer) {
        let mut state = self.state.borrow_mut();
        if state.array_buffer != Some(buffer) {
            state.array_buffer = Some(buffer);
            unsafe { self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer)) };
        }
    }

    /// Write `data` at byte `offset` of `buffer`, through `COPY_WRITE_BUFFER` so no
    /// draw binding changes. The browser copies the bytes from the Wasm memory view.
    pub fn write_buffer(&self, buffer: glow::Buffer, offset: u64, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let mut state = self.state.borrow_mut();
        if state.copy_write_buffer != Some(buffer) {
            state.copy_write_buffer = Some(buffer);
            unsafe { self.gl.bind_buffer(glow::COPY_WRITE_BUFFER, Some(buffer)) };
        }
        unsafe {
            self.gl
                .buffer_sub_data_u8_slice(glow::COPY_WRITE_BUFFER, offset as i32, data)
        };
    }

    /// A buffer of `size` zeroed bytes, typed by its first binding: `target` is
    /// `ARRAY_BUFFER`, `ELEMENT_ARRAY_BUFFER` or `UNIFORM_BUFFER`. WebGL zeroes it in
    /// the GPU process, so no zero vector passes through the Wasm heap.
    pub fn create_buffer(&self, target: u32, size: u64, usage: u32) -> glow::Buffer {
        let buffer = self.created(unsafe { self.gl.create_buffer() }, "buffer");
        match target {
            // An element buffer binds to the bound vertex array; bind none.
            glow::ELEMENT_ARRAY_BUFFER => self.bind_vertex_array(None),
            glow::ARRAY_BUFFER => {
                self.state.borrow_mut().array_buffer = Some(buffer);
            }
            _ => {}
        }
        unsafe {
            self.gl.bind_buffer(target, Some(buffer));
            self.gl.buffer_data_size(target, size as i32, usage);
        }
        buffer
    }

    pub fn bind_uniform_block(&self, point: u32, buffer: glow::Buffer) {
        let mut state = self.state.borrow_mut();
        let slot = &mut state.uniform_blocks[point as usize];
        if *slot != Some(buffer) {
            *slot = Some(buffer);
            unsafe {
                self.gl
                    .bind_buffer_base(glow::UNIFORM_BUFFER, point, Some(buffer))
            };
        }
    }

    fn activate(&self, state: &mut State, unit: u32) {
        if state.active_unit != unit {
            state.active_unit = unit;
            unsafe { self.gl.active_texture(glow::TEXTURE0 + unit) };
        }
    }

    pub fn bind_texture(&self, unit: u32, texture: glow::Texture) {
        let mut state = self.state.borrow_mut();
        if state.textures[unit as usize] != Some(texture) {
            self.activate(&mut state, unit);
            state.textures[unit as usize] = Some(texture);
            unsafe { self.gl.bind_texture(glow::TEXTURE_2D, Some(texture)) };
        }
    }

    pub fn bind_sampler(&self, unit: u32, sampler: Option<glow::Sampler>) {
        let mut state = self.state.borrow_mut();
        if state.samplers[unit as usize] != sampler {
            state.samplers[unit as usize] = sampler;
            unsafe { self.gl.bind_sampler(unit, sampler) };
        }
    }

    /// Bind `texture` on the upload unit for `texImage`/`texParameter` calls.
    pub fn bind_upload_texture(&self, texture: glow::Texture) {
        self.bind_texture(unit::UPLOAD, texture);
        // `bind_texture` skipped activating the unit when the texture was bound already.
        let mut state = self.state.borrow_mut();
        self.activate(&mut state, unit::UPLOAD);
    }

    /// Whether uploads of images flip their rows (`UNPACK_FLIP_Y_WEBGL`); it applies
    /// to array uploads too, so record uploads turn it off.
    pub fn set_flip_y(&self, flip: bool) {
        let mut state = self.state.borrow_mut();
        if state.flip_y != flip {
            state.flip_y = flip;
            unsafe {
                self.gl
                    .pixel_store_bool(web_sys::WebGl2RenderingContext::UNPACK_FLIP_Y_WEBGL, flip)
            };
        }
    }

    /// Bind `framebuffer` (`None`: the canvas) for drawing and reading.
    pub fn bind_framebuffer(&self, framebuffer: Option<glow::Framebuffer>) {
        let mut state = self.state.borrow_mut();
        if state.draw_framebuffer != framebuffer || state.read_framebuffer != framebuffer {
            state.draw_framebuffer = framebuffer;
            state.read_framebuffer = framebuffer;
            unsafe { self.gl.bind_framebuffer(glow::FRAMEBUFFER, framebuffer) };
        }
    }

    /// Copy `mask` (color or depth) of the whole `width` × `height` area from `read`
    /// to `draw`: an MSAA resolve or a depth copy.
    pub fn blit(
        &self,
        read: glow::Framebuffer,
        draw: glow::Framebuffer,
        width: u32,
        height: u32,
        mask: u32,
    ) {
        {
            let mut state = self.state.borrow_mut();
            if state.read_framebuffer != Some(read) {
                state.read_framebuffer = Some(read);
                unsafe { self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(read)) };
            }
            if state.draw_framebuffer != Some(draw) {
                state.draw_framebuffer = Some(draw);
                unsafe { self.gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(draw)) };
            }
        }
        let (w, h) = (width as i32, height as i32);
        unsafe {
            self.gl
                .blit_framebuffer(0, 0, w, h, 0, 0, w, h, mask, glow::NEAREST)
        };
    }

    /// Tell the GPU the read framebuffer's `attachments` need not be kept (WebGPU's
    /// `StoreOp::Discard`), so tilers skip writing them back.
    pub fn invalidate_read(&self, attachments: &[u32]) {
        unsafe {
            self.gl
                .invalidate_framebuffer(glow::READ_FRAMEBUFFER, attachments)
        };
    }

    pub fn viewport(&self, width: u32, height: u32) {
        let viewport = [0, 0, width as i32, height as i32];
        let mut state = self.state.borrow_mut();
        if state.viewport != viewport {
            state.viewport = viewport;
            unsafe { self.gl.viewport(0, 0, viewport[2], viewport[3]) };
        }
    }

    /// Clear the bound framebuffer's color to `color` (if any) and its depth to 1.
    /// Clears obey the depth mask, so depth writes turn on first.
    pub fn clear(&self, color: Option<[f32; 3]>) {
        let mut mask = glow::DEPTH_BUFFER_BIT;
        if let Some([r, g, b]) = color {
            mask |= glow::COLOR_BUFFER_BIT;
            let mut state = self.state.borrow_mut();
            let rgba = [r, g, b, 1.0];
            if state.clear_color != rgba {
                state.clear_color = rgba;
                unsafe { self.gl.clear_color(r, g, b, 1.0) };
            }
        }
        let raster = self.state.borrow().raster;
        if !raster.depth_write {
            self.set_raster(&Raster {
                depth_write: true,
                ..raster
            });
        }
        unsafe { self.gl.clear(mask) };
    }

    /// Apply `raster`, setting only the GL state that differs from the last one.
    pub fn set_raster(&self, raster: &Raster) {
        let mut state = self.state.borrow_mut();
        let last = state.raster;
        if last == *raster {
            return;
        }
        let gl = &self.gl;
        let toggle = |capability: u32, on: bool| unsafe {
            if on {
                gl.enable(capability);
            } else {
                gl.disable(capability);
            }
        };
        if last.cull != raster.cull {
            if (last.cull == Cull::None) != (raster.cull == Cull::None) {
                toggle(glow::CULL_FACE, raster.cull != Cull::None);
            }
            match raster.cull {
                Cull::Back => unsafe { gl.cull_face(glow::BACK) },
                Cull::Front => unsafe { gl.cull_face(glow::FRONT) },
                Cull::None => {}
            }
        }
        if last.depth_func != raster.depth_func {
            if last.depth_func.is_none() != raster.depth_func.is_none() {
                toggle(glow::DEPTH_TEST, raster.depth_func.is_some());
            }
            if let Some(func) = raster.depth_func {
                unsafe { gl.depth_func(func) };
            }
        }
        if last.depth_write != raster.depth_write {
            unsafe { gl.depth_mask(raster.depth_write) };
        }
        if last.blend != raster.blend {
            if (last.blend == Blend::Replace) != (raster.blend == Blend::Replace) {
                toggle(glow::BLEND, raster.blend != Blend::Replace);
            }
            match raster.blend {
                Blend::Normal => unsafe {
                    gl.blend_func_separate(
                        glow::SRC_ALPHA,
                        glow::ONE_MINUS_SRC_ALPHA,
                        glow::ONE,
                        glow::ONE_MINUS_SRC_ALPHA,
                    )
                },
                Blend::Additive => unsafe {
                    gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE, glow::ONE, glow::ONE)
                },
                Blend::Replace => {}
            }
        }
        if last.polygon_offset != raster.polygon_offset {
            let off = |offset: (f32, f32)| offset == (0.0, 0.0);
            if off(last.polygon_offset) != off(raster.polygon_offset) {
                toggle(glow::POLYGON_OFFSET_FILL, !off(raster.polygon_offset));
            }
            if !off(raster.polygon_offset) {
                let (factor, units) = raster.polygon_offset;
                unsafe { gl.polygon_offset(factor, units) };
            }
        }
        if last.alpha_to_coverage != raster.alpha_to_coverage {
            toggle(glow::SAMPLE_ALPHA_TO_COVERAGE, raster.alpha_to_coverage);
        }
        // A feature turned off keeps its mode on the GPU but not in the record, so
        // turning it on again sets the mode too.
        state.raster = *raster;
    }
}
