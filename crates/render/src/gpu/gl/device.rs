//! The state this renderer actually uses. Every mutation (including uploads and
//! deletion) goes through this owner; cached values describe real GL state.
use crate::shader::{BlendMode, DepthBias};
use glow::HasContext;
use sloppy_core::scene::Side;
use std::cell::{Cell, RefCell};
use std::ops::Deref;
use std::rc::Rc;
use wasm_bindgen::{JsCast, JsValue};

const TEXTURE_UNITS: usize = 16;
// getError synchronizes with the browser's GL process. Bound error latency without
// putting that round trip on every draw; setup and resource checks stay immediate.
const ERROR_POLL_FRAMES: u32 = 30;
pub const UPLOAD_UNIT: u32 = 15;
#[derive(Clone)]
pub struct Device(Rc<Inner>);
pub type Queue = Device;
pub struct Inner {
    pub gl: glow::Context,
    pub raw: web_sys::WebGl2RenderingContext,
    state: RefCell<State>,
    error: RefCell<Option<String>>,
    frames_since_error_check: Cell<u32>,
    pub max_texture: u32,
    pub anisotropy: f32,
    vao: glow::VertexArray,
}
impl Deref for Device {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}
#[derive(Clone, Copy, PartialEq)]
struct Attribute {
    buffer: glow::Buffer,
    size: i32,
    stride: i32,
    offset: i32,
    integer: bool,
    divisor: u32,
}
#[derive(Clone, Copy, PartialEq)]
pub struct Raster {
    pub side: Side,
    pub blend: BlendMode,
    pub depth_test: bool,
    pub depth_write: bool,
    pub bias: DepthBias,
    pub coverage: bool,
}
impl Default for Raster {
    fn default() -> Self {
        Self {
            side: Side::Front,
            blend: BlendMode::Replace,
            depth_test: true,
            depth_write: true,
            bias: DepthBias::default(),
            coverage: false,
        }
    }
}
struct State {
    program: Option<glow::Program>,
    array: Option<glow::Buffer>,
    index: Option<glow::Buffer>,
    uniform: Option<glow::Buffer>,
    blocks: [Option<glow::Buffer>; 2],
    active: u32,
    textures: [Option<glow::Texture>; TEXTURE_UNITS],
    samplers: [Option<glow::Sampler>; TEXTURE_UNITS],
    read: Option<glow::Framebuffer>,
    draw: Option<glow::Framebuffer>,
    viewport: (u32, u32),
    attributes: [Option<Attribute>; 6],
    raster: Option<Raster>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            program: None,
            array: None,
            index: None,
            uniform: None,
            blocks: [None; 2],
            active: 0,
            textures: [None; TEXTURE_UNITS],
            samplers: [None; TEXTURE_UNITS],
            read: None,
            draw: None,
            viewport: (0, 0),
            attributes: [None; 6],
            raster: None,
        }
    }
}
impl Device {
    pub fn new(canvas: &web_sys::HtmlCanvasElement) -> Result<Self, String> {
        let options = js_sys::Object::new();
        for (key, value) in [
            ("alpha", false),
            ("antialias", false),
            ("premultipliedAlpha", false),
            ("preserveDrawingBuffer", false),
        ] {
            js_sys::Reflect::set(&options, &key.into(), &value.into())
                .map_err(|e| format!("WebGL options: {e:?}"))?;
        }
        js_sys::Reflect::set(
            &options,
            &"powerPreference".into(),
            &"high-performance".into(),
        )
        .map_err(|e| format!("WebGL options: {e:?}"))?;
        let raw: web_sys::WebGl2RenderingContext = canvas
            .get_context_with_context_options("webgl2", &options)
            .map_err(|e| format!("WebGL canvas unavailable: {e:?}"))?
            .ok_or("WebGL canvas unavailable")?
            .dyn_into()
            .map_err(|_| "WebGL2 context unavailable")?;
        if raw
            .get_extension("EXT_color_buffer_float")
            .ok()
            .flatten()
            .is_none()
        {
            return Err("WebGL requires EXT_color_buffer_float for the HDR renderer".into());
        }
        let anisotropy = if raw
            .get_extension("EXT_texture_filter_anisotropic")
            .ok()
            .flatten()
            .is_some()
        {
            raw.get_parameter(0x84ff)
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(1.) as f32
        } else {
            1.
        };
        let gl = glow::Context::from_webgl2_context(raw.clone());
        unsafe {
            let max_texture = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE) as u32;
            let vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao));
            // Naga maps WebGPU's upper-left origin into GL by flipping clip Y.
            gl.front_face(glow::CW);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            gl.pixel_store_i32(
                web_sys::WebGl2RenderingContext::UNPACK_COLORSPACE_CONVERSION_WEBGL,
                glow::NONE as i32,
            );
            Ok(Self(Rc::new(Inner {
                gl,
                raw,
                state: RefCell::default(),
                error: RefCell::default(),
                frames_since_error_check: Cell::new(0),
                max_texture,
                anisotropy,
                vao,
            })))
        }
    }
    pub fn fail(&self, message: String) {
        let mut slot = self.error.borrow_mut();
        if slot.is_none() {
            web_sys::console::error_1(&JsValue::from_str(&message));
            *slot = Some(message);
        }
    }
    pub fn error(&self) -> Option<String> {
        if self.raw.is_context_lost() {
            self.fail("WebGL context lost. Reload to restart.".into());
        }
        self.error.borrow().clone()
    }
    pub fn check(&self, label: &str) {
        unsafe {
            let error = self.gl.get_error();
            if error != glow::NO_ERROR {
                self.fail(format!("WebGL {label}: GL error 0x{error:04x}"));
            }
        }
    }
    pub fn check_frame(&self) {
        let frames = self.frames_since_error_check.get() + 1;
        if frames == ERROR_POLL_FRAMES {
            self.frames_since_error_check.set(0);
            self.check("frame");
        } else {
            self.frames_since_error_check.set(frames);
        }
    }
    pub fn program(&self, program: glow::Program) {
        let mut s = self.state.borrow_mut();
        if s.program != Some(program) {
            unsafe {
                self.gl.use_program(Some(program));
            }
            s.program = Some(program);
        }
    }
    pub fn buffer(&self, target: u32, buffer: glow::Buffer) {
        let mut s = self.state.borrow_mut();
        let slot = match target {
            glow::ARRAY_BUFFER => &mut s.array,
            glow::ELEMENT_ARRAY_BUFFER => &mut s.index,
            glow::UNIFORM_BUFFER => &mut s.uniform,
            _ => unreachable!("renderer buffer target"),
        };
        if *slot != Some(buffer) {
            unsafe {
                self.gl.bind_buffer(target, Some(buffer));
            }
            *slot = Some(buffer);
        }
    }
    pub fn block(&self, index: usize, buffer: &Buffer) {
        let mut s = self.state.borrow_mut();
        let b = buffer.raw();
        if s.blocks[index] != Some(b) {
            unsafe {
                self.gl
                    .bind_buffer_base(glow::UNIFORM_BUFFER, index as u32, Some(b));
            }
            s.blocks[index] = Some(b);
            s.uniform = Some(b);
        }
    }
    pub fn texture(&self, unit: u32, texture: &Texture, sampler: Option<&Sampler>) {
        let mut s = self.state.borrow_mut();
        let i = unit as usize;
        let t = texture.raw();
        if s.textures[i] != Some(t) {
            if s.active != unit {
                unsafe {
                    self.gl.active_texture(glow::TEXTURE0 + unit);
                }
                s.active = unit;
            }
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(t));
            }
            s.textures[i] = Some(t);
        }
        let sampler = sampler.map(|s| s.0.raw);
        if s.samplers[i] != sampler {
            unsafe {
                self.gl.bind_sampler(unit, sampler);
            }
            s.samplers[i] = sampler;
        }
    }
    pub fn upload(&self, texture: &Texture) {
        self.texture(UPLOAD_UNIT, texture, None);
        let mut s = self.state.borrow_mut();
        if s.active != UPLOAD_UNIT {
            unsafe {
                self.gl.active_texture(glow::TEXTURE0 + UPLOAD_UNIT);
            }
            s.active = UPLOAD_UNIT;
        }
    }
    pub fn framebuffers(&self, read: Option<glow::Framebuffer>, draw: Option<glow::Framebuffer>) {
        let mut s = self.state.borrow_mut();
        unsafe {
            if s.read != read {
                self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, read);
                s.read = read;
            }
            if s.draw != draw {
                self.gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, draw);
                s.draw = draw;
            }
        }
    }
    pub fn target(&self, fb: Option<glow::Framebuffer>, width: u32, height: u32) {
        self.framebuffers(fb, fb);
        let mut s = self.state.borrow_mut();
        if s.viewport != (width, height) {
            unsafe {
                self.gl.viewport(0, 0, width as i32, height as i32);
            }
            s.viewport = (width, height);
        }
    }
    pub fn check_framebuffer(&self, label: &str) {
        unsafe {
            let status = self.gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                self.fail(format!(
                    "WebGL {label}: incomplete framebuffer 0x{status:04x}"
                ));
            }
        }
    }
    pub fn raster(&self, next: Raster) {
        let mut s = self.state.borrow_mut();
        let old = s.raster;
        unsafe {
            let toggle = |cap, on| {
                if on {
                    self.gl.enable(cap)
                } else {
                    self.gl.disable(cap)
                }
            };
            if old.is_none_or(|o| o.side != next.side) {
                toggle(glow::CULL_FACE, next.side != Side::Double);
                if next.side != Side::Double {
                    self.gl.cull_face(if next.side == Side::Front {
                        glow::BACK
                    } else {
                        glow::FRONT
                    });
                }
            }
            if old.is_none_or(|o| o.blend != next.blend) {
                toggle(glow::BLEND, next.blend != BlendMode::Replace);
                if next.blend != BlendMode::Replace {
                    self.gl.blend_equation(glow::FUNC_ADD);
                    let dst = if next.blend == BlendMode::Normal {
                        glow::ONE_MINUS_SRC_ALPHA
                    } else {
                        glow::ONE
                    };
                    self.gl
                        .blend_func_separate(glow::SRC_ALPHA, dst, glow::ONE, dst);
                }
            }
            if old.is_none_or(|o| o.depth_test != next.depth_test) {
                self.gl.enable(glow::DEPTH_TEST);
                self.gl.depth_func(if next.depth_test {
                    glow::LEQUAL
                } else {
                    glow::ALWAYS
                });
            }
            if old.is_none_or(|o| o.depth_write != next.depth_write) {
                self.gl.depth_mask(next.depth_write);
            }
            if old.is_none_or(|o| o.bias != next.bias) {
                let enabled = next.bias != DepthBias::default();
                toggle(glow::POLYGON_OFFSET_FILL, enabled);
                if enabled {
                    self.gl
                        .polygon_offset(next.bias.slope_scale(), next.bias.constant as f32);
                }
            }
            if old.is_none_or(|o| o.coverage != next.coverage) {
                toggle(glow::SAMPLE_ALPHA_TO_COVERAGE, next.coverage);
            }
        }
        s.raster = Some(next);
    }
    pub fn clear(&self, color: Option<[f32; 4]>, depth: bool) {
        // A depth clear observes DEPTH_WRITEMASK. Restore it and the cache together.
        if depth {
            let mut s = self.state.borrow_mut();
            unsafe {
                self.gl.depth_mask(true);
                self.gl.clear_depth_f32(1.);
            }
            if let Some(r) = &mut s.raster {
                r.depth_write = true;
            }
        }
        unsafe {
            let mut mask = if depth { glow::DEPTH_BUFFER_BIT } else { 0 };
            if let Some(c) = color {
                self.gl.clear_color(c[0], c[1], c[2], c[3]);
                mask |= glow::COLOR_BUFFER_BIT;
            }
            self.gl.clear(mask);
        }
    }
    pub fn vertices(
        &self,
        vertex: &Buffer,
        extra: Option<&Buffer>,
        bases: Option<(&Buffer, u32)>,
        extras: u8,
        merged: bool,
    ) {
        let mut desired = [None; 6];
        let attribute = |buffer: &Buffer, size, stride, offset, integer, divisor| {
            Some(Attribute {
                buffer: buffer.raw(),
                size,
                stride,
                offset,
                integer,
                divisor,
            })
        };
        if merged {
            desired[0] = attribute(vertex, 3, 24, 0, false, 0);
            desired[1] = attribute(vertex, 1, 24, 12, true, 0);
            desired[3] = attribute(vertex, 2, 24, 16, false, 0);
            if let Some((buffer, first)) = bases {
                desired[2] = attribute(buffer, 1, 4, (first * 4) as i32, true, 1);
            }
        } else {
            for (i, (size, offset)) in [(3, 0), (3, 12), (2, 24), (4, 32)].into_iter().enumerate() {
                desired[i] = attribute(vertex, size, 48, offset, false, 0);
            }
            if let Some(extra) = extra {
                for i in 0..extras as usize {
                    desired[4 + i] =
                        attribute(extra, 4, 16 * extras as i32, 16 * i as i32, false, 0);
                }
            }
        }
        self.attributes(desired);
    }
    pub fn no_vertices(&self) {
        self.attributes([None; 6]);
    }
    fn attributes(&self, desired: [Option<Attribute>; 6]) {
        for (i, a) in desired.into_iter().enumerate() {
            let old = self.state.borrow().attributes[i];
            if old == a {
                continue;
            }
            unsafe {
                if let Some(a) = a {
                    self.buffer(glow::ARRAY_BUFFER, a.buffer);
                    if a.integer {
                        self.gl.vertex_attrib_pointer_i32(
                            i as u32,
                            a.size,
                            glow::UNSIGNED_INT,
                            a.stride,
                            a.offset,
                        );
                    } else {
                        self.gl.vertex_attrib_pointer_f32(
                            i as u32,
                            a.size,
                            glow::FLOAT,
                            false,
                            a.stride,
                            a.offset,
                        );
                    }
                    self.gl.vertex_attrib_divisor(i as u32, a.divisor);
                    if old.is_none() {
                        self.gl.enable_vertex_attrib_array(i as u32);
                    }
                } else {
                    self.gl.disable_vertex_attrib_array(i as u32);
                }
            }
            self.state.borrow_mut().attributes[i] = a;
        }
    }
    pub fn delete_program(&self, p: glow::Program) {
        let mut s = self.state.borrow_mut();
        if s.program == Some(p) {
            unsafe {
                self.gl.use_program(None);
            }
            s.program = None;
        }
        unsafe {
            self.gl.delete_program(p);
        }
    }
    fn delete_buffer(&self, b: glow::Buffer) {
        let mut s = self.state.borrow_mut();
        let State {
            array,
            index,
            uniform,
            ..
        } = &mut *s;
        for slot in [array, index, uniform] {
            if *slot == Some(b) {
                *slot = None;
            }
        }
        for slot in &mut s.blocks {
            if *slot == Some(b) {
                *slot = None;
            }
        }
        for (i, a) in s.attributes.iter_mut().enumerate() {
            if a.is_some_and(|a| a.buffer == b) {
                unsafe {
                    self.gl.disable_vertex_attrib_array(i as u32);
                }
                *a = None;
            }
        }
        unsafe {
            self.gl.delete_buffer(b);
        }
    }
    fn delete_texture(&self, t: glow::Texture) {
        let mut s = self.state.borrow_mut();
        for slot in &mut s.textures {
            if *slot == Some(t) {
                *slot = None;
            }
        }
        unsafe {
            self.gl.delete_texture(t);
        }
    }
    fn delete_sampler(&self, t: glow::Sampler) {
        let mut s = self.state.borrow_mut();
        for slot in &mut s.samplers {
            if *slot == Some(t) {
                *slot = None;
            }
        }
        unsafe {
            self.gl.delete_sampler(t);
        }
    }
    pub fn delete_framebuffer(&self, f: glow::Framebuffer) {
        let mut s = self.state.borrow_mut();
        if s.read == Some(f) {
            s.read = None;
        }
        if s.draw == Some(f) {
            s.draw = None;
        }
        unsafe {
            self.gl.delete_framebuffer(f);
        }
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        unsafe {
            self.gl.delete_vertex_array(self.vao);
        }
    }
}

struct BufferObject {
    device: Device,
    raw: Cell<Option<glow::Buffer>>,
    target: u32,
}
#[derive(Clone)]
pub struct Buffer(Rc<BufferObject>);
impl Buffer {
    pub fn new(device: &Device, target: u32, size: u64) -> Self {
        unsafe {
            let raw = device.gl.create_buffer().expect("GL buffer");
            device.buffer(target, raw);
            device.gl.buffer_data_size(
                target,
                i32::try_from(size).expect("bounded buffer size"),
                glow::DYNAMIC_DRAW,
            );
            Self(Rc::new(BufferObject {
                device: device.clone(),
                raw: Cell::new(Some(raw)),
                target,
            }))
        }
    }
    pub fn raw(&self) -> glow::Buffer {
        self.0.raw.get().expect("live buffer")
    }
    pub fn destroy(&self) {
        if let Some(raw) = self.0.raw.take() {
            self.0.device.delete_buffer(raw);
        }
    }
}
impl Drop for BufferObject {
    fn drop(&mut self) {
        if let Some(raw) = self.raw.take() {
            self.device.delete_buffer(raw);
        }
    }
}
pub fn write_buffer(queue: &Queue, buffer: &Buffer, offset: u64, data: &[u8]) {
    if data.is_empty() {
        return;
    }
    queue.buffer(buffer.0.target, buffer.raw());
    unsafe {
        queue
            .gl
            .buffer_sub_data_u8_slice(buffer.0.target, offset as i32, data);
    }
}
pub fn write_material_uniform(device: &Device, buffer: &Buffer, data: &[u8]) {
    write_buffer(device, buffer, 0, data);
}
pub fn uniform_buffer(device: &Device, _label: &str, size: u64) -> Buffer {
    Buffer::new(device, glow::UNIFORM_BUFFER, size)
}
pub fn base_buffer(device: &Device, capacity: u32) -> Buffer {
    Buffer::new(device, glow::ARRAY_BUFFER, capacity as u64 * 4)
}
pub fn mesh_buffer(device: &Device, _label: &str, size: u64, index: bool) -> Buffer {
    Buffer::new(
        device,
        if index {
            glow::ELEMENT_ARRAY_BUFFER
        } else {
            glow::ARRAY_BUFFER
        },
        size,
    )
}

struct TextureObject {
    device: Device,
    raw: Cell<Option<glow::Texture>>,
}
#[derive(Clone)]
pub struct Texture(Rc<TextureObject>);
impl Texture {
    pub fn new(device: &Device, format: u32, width: u32, height: u32, levels: u32) -> Self {
        unsafe {
            let raw = device.gl.create_texture().expect("GL texture");
            let texture = Self(Rc::new(TextureObject {
                device: device.clone(),
                raw: Cell::new(Some(raw)),
            }));
            device.texture(UPLOAD_UNIT, &texture, None);
            device.gl.tex_storage_2d(
                glow::TEXTURE_2D,
                levels as i32,
                format,
                width as i32,
                height as i32,
            );
            device.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            device.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
            device.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            device.gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            texture
        }
    }
    pub fn raw(&self) -> glow::Texture {
        self.0.raw.get().expect("live texture")
    }
    pub fn destroy(&self) {
        if let Some(raw) = self.0.raw.take() {
            self.0.device.delete_texture(raw);
        }
    }
}
impl Drop for TextureObject {
    fn drop(&mut self) {
        if let Some(raw) = self.raw.take() {
            self.device.delete_texture(raw);
        }
    }
}
struct SamplerObject {
    device: Device,
    raw: glow::Sampler,
}
#[derive(Clone)]
pub struct Sampler(Rc<SamplerObject>);
impl Sampler {
    pub fn new(
        device: &Device,
        linear: bool,
        mips: bool,
        wrap: u32,
        compare: bool,
        anisotropy: f32,
    ) -> Self {
        unsafe {
            let raw = device.gl.create_sampler().expect("GL sampler");
            let gl = &device.gl;
            gl.sampler_parameter_i32(
                raw,
                glow::TEXTURE_MIN_FILTER,
                if mips {
                    glow::LINEAR_MIPMAP_LINEAR
                } else if linear {
                    glow::LINEAR
                } else {
                    glow::NEAREST
                } as i32,
            );
            gl.sampler_parameter_i32(
                raw,
                glow::TEXTURE_MAG_FILTER,
                if linear { glow::LINEAR } else { glow::NEAREST } as i32,
            );
            for axis in [glow::TEXTURE_WRAP_S, glow::TEXTURE_WRAP_T] {
                gl.sampler_parameter_i32(raw, axis, wrap as i32);
            }
            if compare {
                gl.sampler_parameter_i32(
                    raw,
                    glow::TEXTURE_COMPARE_MODE,
                    glow::COMPARE_REF_TO_TEXTURE as i32,
                );
                gl.sampler_parameter_i32(raw, glow::TEXTURE_COMPARE_FUNC, glow::LEQUAL as i32);
            }
            if device.anisotropy > 1. {
                gl.sampler_parameter_f32(raw, 0x84fe, anisotropy.clamp(1., device.anisotropy));
            }
            Self(Rc::new(SamplerObject {
                device: device.clone(),
                raw,
            }))
        }
    }
}
impl Drop for SamplerObject {
    fn drop(&mut self) {
        self.device.delete_sampler(self.raw);
    }
}
