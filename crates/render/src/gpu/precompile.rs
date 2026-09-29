//! Background pipeline compilation.
//!
//! wgpu creates render pipelines with the browser's synchronous
//! `GPUDevice.createRenderPipeline`, which Chrome compiles on the GPU process's main
//! thread. With a cold system shader cache an arena's pipelines take seconds there,
//! and the whole browser stalls meanwhile: no animation frames or timers in this or
//! other tabs. `createRenderPipelineAsync` compiles on background threads instead.
//!
//! So every pipeline is first described once as a [`PipelineSpec`]. The precompiler
//! issues `createRenderPipelineAsync` for the browser descriptor that wgpu's WebGPU
//! backend would build from it (same WGSL, bind group layouts, vertex buffers, states
//! and targets); once that resolves, wgpu creates the identical pipeline, which the
//! browser's pipeline and shader caches now answer at once. The mapping below mirrors
//! wgpu 30's `backend/webgpu.rs`: a descriptor that differs only costs the stall
//! again, since the pipeline drawn with is always wgpu's own.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::rc::Rc;

use js_sys::{Array, Object, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

/// Pipelines compiling at once. Background compiles scale well with parallelism
/// (30 cold pipelines: 6.4 s one at a time, 1.2 s eight at a time, 0.95 s all at
/// once on an M-series Mac); eight leaves cores for the page and other tabs.
const CONCURRENCY: usize = 8;

#[wasm_bindgen]
extern "C" {
    /// The browser `GPUDevice` behind wgpu's device.
    #[derive(Clone)]
    type RawDevice;

    #[wasm_bindgen(method, js_name = createShaderModule)]
    fn create_shader_module(this: &RawDevice, descriptor: &Object) -> JsValue;

    #[wasm_bindgen(method, js_name = createBindGroupLayout)]
    fn create_bind_group_layout(this: &RawDevice, descriptor: &Object) -> JsValue;

    #[wasm_bindgen(method, js_name = createPipelineLayout)]
    fn create_pipeline_layout(this: &RawDevice, descriptor: &Object) -> JsValue;

    #[wasm_bindgen(method, js_name = createRenderPipelineAsync)]
    fn create_render_pipeline_async(this: &RawDevice, descriptor: &Object) -> Promise;
}

/// The bind groups a pipeline layout is made of, shared by wgpu's layouts and the
/// precompiler's copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutKind {
    /// Frame and material groups: surface, shadow and alpha-tested caster pipelines.
    Surface,
    Water,
    Output,
    /// Frame group only: depth-only merged shadow casters.
    ShadowMerged,
    Mipmap,
}

impl LayoutKind {
    pub fn groups(self) -> &'static [&'static [wgpu::BindGroupLayoutEntry]] {
        use super::resources::{FRAME_ENTRIES, MATERIAL_ENTRIES, OUTPUT_ENTRIES, TEXTURED_ENTRIES};
        use super::textures::MIPMAP_SOURCE_ENTRIES;
        match self {
            LayoutKind::Surface => &[FRAME_ENTRIES, MATERIAL_ENTRIES],
            LayoutKind::Water => &[FRAME_ENTRIES, TEXTURED_ENTRIES],
            LayoutKind::Output => &[OUTPUT_ENTRIES],
            LayoutKind::ShadowMerged => &[FRAME_ENTRIES],
            LayoutKind::Mipmap => &[MIPMAP_SOURCE_ENTRIES],
        }
    }
}

/// Everything a render pipeline is made from except its shader module, whose one
/// WGSL source makes both wgpu's module and the precompiler's. Vertex and fragment
/// stages share the module.
#[derive(Clone, Debug)]
pub struct PipelineSpec {
    pub label: &'static str,
    pub layout: LayoutKind,
    pub vertex_entry: &'static str,
    pub fragment_entry: Option<&'static str>,
    pub buffers: Vec<wgpu::VertexBufferLayout<'static>>,
    pub targets: Vec<Option<wgpu::ColorTargetState>>,
    pub primitive: wgpu::PrimitiveState,
    pub depth_stencil: Option<wgpu::DepthStencilState>,
    pub multisample: wgpu::MultisampleState,
}

impl PipelineSpec {
    /// The wgpu pipeline; instant once [`Precompiler`] has compiled the same one.
    pub fn create(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        module: &wgpu::ShaderModule,
    ) -> wgpu::RenderPipeline {
        let buffers: Vec<_> = self.buffers.iter().cloned().map(Some).collect();
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(self.label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module,
                entry_point: Some(self.vertex_entry),
                compilation_options: Default::default(),
                buffers: &buffers,
            },
            primitive: self.primitive,
            depth_stencil: self.depth_stencil.clone(),
            multisample: self.multisample,
            fragment: self.fragment_entry.map(|entry| wgpu::FragmentState {
                module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &self.targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    }
}

/// A browser shader module made from the same WGSL as a wgpu one.
#[derive(Clone)]
pub struct RawModule(JsValue);

struct Queue<K> {
    /// Keys ever queued (a key compiles once per page).
    started: HashSet<K>,
    waiting: VecDeque<(K, Object)>,
    running: usize,
    /// Compiled pipelines, held so the browser's cache keeps them until wgpu asks.
    finished: HashMap<K, JsValue>,
}

/// Compiles pipelines on the GPU process's background threads, at most
/// [`CONCURRENCY`] at a time, keyed by `K`.
pub struct Precompiler<K> {
    device: RawDevice,
    layouts: RefCell<HashMap<LayoutKind, JsValue>>,
    queue: Rc<RefCell<Queue<K>>>,
}

impl<K: Copy + Eq + Hash + 'static> Precompiler<K> {
    pub fn new(device: &wgpu::Device) -> Self {
        let device = device
            .as_webgpu()
            .expect("the renderer runs on the browser's WebGPU")
            .unchecked_ref::<RawDevice>()
            .clone();
        Self {
            device,
            layouts: RefCell::default(),
            queue: Rc::new(RefCell::new(Queue {
                started: HashSet::new(),
                waiting: VecDeque::new(),
                running: 0,
                finished: HashMap::new(),
            })),
        }
    }

    pub fn module(&self, label: &str, source: &str) -> RawModule {
        let descriptor = object(&[("label", label.into()), ("code", source.into())]);
        RawModule(self.device.create_shader_module(&descriptor))
    }

    /// Queue `key` for compiling unless it was queued before.
    pub fn start(&self, key: K, spec: &PipelineSpec, module: &RawModule) {
        if !self.queue.borrow_mut().started.insert(key) {
            return;
        }
        match self.descriptor(spec, module) {
            Ok(descriptor) => self.queue.borrow_mut().waiting.push_back((key, descriptor)),
            Err(error) => {
                // wgpu compiles it synchronously, as before.
                web_sys::console::warn_1(
                    &format!("Cannot precompile the {} pipeline: {error}", spec.label).into(),
                );
                self.queue.borrow_mut().finished.insert(key, JsValue::NULL);
            }
        }
        self.pump();
    }

    /// Whether `key` was ever queued.
    pub fn queued(&self, key: &K) -> bool {
        self.queue.borrow().started.contains(key)
    }

    /// Whether `key` was compiled; wgpu may create it now without stalling.
    pub fn finished(&self, key: &K) -> bool {
        self.queue.borrow().finished.contains_key(key)
    }

    /// Release the compiled copy once wgpu holds the pipeline.
    pub fn release(&self, key: &K) {
        self.queue.borrow_mut().finished.remove(key);
    }

    /// Pipelines waiting for or being compiled.
    pub fn compiling(&self) -> u32 {
        let queue = self.queue.borrow();
        (queue.waiting.len() + queue.running) as u32
    }

    fn pump(&self) {
        let mut queue = self.queue.borrow_mut();
        while queue.running < CONCURRENCY && queue.running < queue.waiting.len() {
            queue.running += 1;
            let (shared, device) = (self.queue.clone(), self.device.clone());
            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    let job = shared.borrow_mut().waiting.pop_front();
                    let Some((key, descriptor)) = job else { break };
                    let promise = device.create_render_pipeline_async(&descriptor);
                    let pipeline = JsFuture::from(promise).await.unwrap_or_else(|error| {
                        // wgpu's own creation reports the error through the device.
                        web_sys::console::warn_2(&"Pipeline precompile failed:".into(), &error);
                        JsValue::NULL
                    });
                    shared.borrow_mut().finished.insert(key, pipeline);
                }
                shared.borrow_mut().running -= 1;
            });
        }
    }

    /// Compile every spec in the background and wait for all of them; for the few
    /// fixed pipelines created with the renderer. Hold the returned pipelines until
    /// wgpu has created its own, so the browser's cache still has them.
    pub async fn compile_all(&self, specs: &[(&PipelineSpec, &RawModule)]) -> Vec<JsValue> {
        let mut promises = Vec::with_capacity(specs.len());
        for (spec, module) in specs {
            match self.descriptor(spec, module) {
                Ok(descriptor) => {
                    promises.push(self.device.create_render_pipeline_async(&descriptor))
                }
                Err(error) => web_sys::console::warn_1(
                    &format!("Cannot precompile the {} pipeline: {error}", spec.label).into(),
                ),
            }
        }
        let mut compiled = Vec::with_capacity(promises.len());
        for promise in promises {
            match JsFuture::from(promise).await {
                Ok(pipeline) => compiled.push(pipeline),
                Err(error) => {
                    web_sys::console::warn_2(&"Pipeline precompile failed:".into(), &error)
                }
            }
        }
        compiled
    }

    fn layout(&self, kind: LayoutKind) -> Result<JsValue, String> {
        if let Some(layout) = self.layouts.borrow().get(&kind) {
            return Ok(layout.clone());
        }
        let groups = kind
            .groups()
            .iter()
            .map(|entries| {
                let entries = entries
                    .iter()
                    .map(bind_group_layout_entry)
                    .collect::<Result<Array, _>>()?;
                let descriptor = object(&[("entries", entries.into())]);
                Ok(self.device.create_bind_group_layout(&descriptor))
            })
            .collect::<Result<Array, String>>()?;
        let layout = self
            .device
            .create_pipeline_layout(&object(&[("bindGroupLayouts", groups.into())]));
        self.layouts.borrow_mut().insert(kind, layout.clone());
        Ok(layout)
    }

    /// The browser descriptor wgpu builds for `spec` (wgpu 30 `create_render_pipeline`).
    fn descriptor(&self, spec: &PipelineSpec, module: &RawModule) -> Result<Object, String> {
        let buffers = spec
            .buffers
            .iter()
            .map(|buffer| {
                let attributes = buffer
                    .attributes
                    .iter()
                    .map(|attribute| {
                        Ok::<JsValue, String>(
                            object(&[
                                ("format", vertex_format(attribute.format)?.into()),
                                ("offset", (attribute.offset as f64).into()),
                                ("shaderLocation", attribute.shader_location.into()),
                            ])
                            .into(),
                        )
                    })
                    .collect::<Result<Array, String>>()?;
                Ok::<JsValue, String>(
                    object(&[
                        ("arrayStride", (buffer.array_stride as f64).into()),
                        ("attributes", attributes.into()),
                        (
                            "stepMode",
                            match buffer.step_mode {
                                wgpu::VertexStepMode::Vertex => "vertex",
                                wgpu::VertexStepMode::Instance => "instance",
                            }
                            .into(),
                        ),
                    ])
                    .into(),
                )
            })
            .collect::<Result<Array, String>>()?;
        let vertex = object(&[
            ("module", module.0.clone()),
            ("entryPoint", spec.vertex_entry.into()),
            ("buffers", buffers.into()),
        ]);
        let descriptor = object(&[
            ("label", spec.label.into()),
            ("layout", self.layout(spec.layout)?),
            ("vertex", vertex.into()),
        ]);
        if let Some(depth) = &spec.depth_stencil {
            set(&descriptor, "depthStencil", depth_stencil(depth)?.into());
        }
        if let Some(entry) = spec.fragment_entry {
            let targets = spec
                .targets
                .iter()
                .map(|target| match target {
                    Some(target) => color_target(target).map(JsValue::from),
                    None => Ok(JsValue::NULL),
                })
                .collect::<Result<Array, String>>()?;
            let fragment = object(&[
                ("module", module.0.clone()),
                ("entryPoint", entry.into()),
                ("targets", targets.into()),
            ]);
            set(&descriptor, "fragment", fragment.into());
        }
        let multisample = object(&[
            ("count", spec.multisample.count.into()),
            ("mask", (spec.multisample.mask as u32).into()),
            (
                "alphaToCoverageEnabled",
                spec.multisample.alpha_to_coverage_enabled.into(),
            ),
        ]);
        set(&descriptor, "multisample", multisample.into());
        set(&descriptor, "primitive", primitive(&spec.primitive)?.into());
        Ok(descriptor)
    }
}

fn object(fields: &[(&str, JsValue)]) -> Object {
    let object = Object::new();
    for (name, value) in fields {
        set(&object, name, value.clone());
    }
    object
}

fn set(object: &Object, name: &str, value: JsValue) {
    Reflect::set(object, &name.into(), &value).expect("plain objects accept properties");
}

fn bind_group_layout_entry(entry: &wgpu::BindGroupLayoutEntry) -> Result<JsValue, String> {
    if entry.count.is_some() {
        return Err("binding arrays".into());
    }
    let mapped = object(&[
        ("binding", entry.binding.into()),
        ("visibility", entry.visibility.bits().into()),
    ]);
    match entry.ty {
        wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset,
            min_binding_size,
        } => {
            let buffer = object(&[
                ("hasDynamicOffset", has_dynamic_offset.into()),
                (
                    "type",
                    match ty {
                        wgpu::BufferBindingType::Uniform => "uniform",
                        wgpu::BufferBindingType::Storage { read_only: false } => "storage",
                        wgpu::BufferBindingType::Storage { read_only: true } => "read-only-storage",
                    }
                    .into(),
                ),
            ]);
            if let Some(size) = min_binding_size {
                set(&buffer, "minBindingSize", (size.get() as f64).into());
            }
            set(&mapped, "buffer", buffer.into());
        }
        wgpu::BindingType::Sampler(ty) => {
            let ty = match ty {
                wgpu::SamplerBindingType::NonFiltering => "non-filtering",
                wgpu::SamplerBindingType::Filtering => "filtering",
                wgpu::SamplerBindingType::Comparison => "comparison",
            };
            set(&mapped, "sampler", object(&[("type", ty.into())]).into());
        }
        wgpu::BindingType::Texture {
            multisampled,
            sample_type,
            view_dimension,
        } => {
            let sample_type = match sample_type {
                wgpu::TextureSampleType::Float { filterable: true } => "float",
                wgpu::TextureSampleType::Float { filterable: false } => "unfilterable-float",
                wgpu::TextureSampleType::Sint => "sint",
                wgpu::TextureSampleType::Uint => "uint",
                wgpu::TextureSampleType::Depth => "depth",
            };
            let texture = object(&[
                ("multisampled", multisampled.into()),
                ("sampleType", sample_type.into()),
                ("viewDimension", view_dimension_name(view_dimension).into()),
            ]);
            set(&mapped, "texture", texture.into());
        }
        other => return Err(format!("binding type {other:?}")),
    }
    Ok(mapped.into())
}

fn view_dimension_name(dimension: wgpu::TextureViewDimension) -> &'static str {
    match dimension {
        wgpu::TextureViewDimension::D1 => "1d",
        wgpu::TextureViewDimension::D2 => "2d",
        wgpu::TextureViewDimension::D2Array => "2d-array",
        wgpu::TextureViewDimension::Cube => "cube",
        wgpu::TextureViewDimension::CubeArray => "cube-array",
        wgpu::TextureViewDimension::D3 => "3d",
    }
}

fn vertex_format(format: wgpu::VertexFormat) -> Result<&'static str, String> {
    use wgpu::VertexFormat as F;
    Ok(match format {
        F::Float32 => "float32",
        F::Float32x2 => "float32x2",
        F::Float32x3 => "float32x3",
        F::Float32x4 => "float32x4",
        F::Uint32 => "uint32",
        F::Uint32x2 => "uint32x2",
        F::Uint32x3 => "uint32x3",
        F::Uint32x4 => "uint32x4",
        F::Sint32 => "sint32",
        F::Sint32x2 => "sint32x2",
        F::Sint32x3 => "sint32x3",
        F::Sint32x4 => "sint32x4",
        F::Float16x2 => "float16x2",
        F::Float16x4 => "float16x4",
        F::Unorm8x4 => "unorm8x4",
        F::Snorm8x4 => "snorm8x4",
        F::Uint8x4 => "uint8x4",
        F::Unorm16x2 => "unorm16x2",
        F::Unorm16x4 => "unorm16x4",
        other => return Err(format!("vertex format {other:?}")),
    })
}

fn texture_format(format: wgpu::TextureFormat) -> Result<&'static str, String> {
    use wgpu::TextureFormat as F;
    Ok(match format {
        F::Rgba8Unorm => "rgba8unorm",
        F::Rgba8UnormSrgb => "rgba8unorm-srgb",
        F::Bgra8Unorm => "bgra8unorm",
        F::Bgra8UnormSrgb => "bgra8unorm-srgb",
        F::Rgba16Float => "rgba16float",
        F::Rg16Float => "rg16float",
        F::R16Float => "r16float",
        F::Rgba32Float => "rgba32float",
        F::Depth32Float => "depth32float",
        F::Depth24Plus => "depth24plus",
        F::Depth24PlusStencil8 => "depth24plus-stencil8",
        other => return Err(format!("texture format {other:?}")),
    })
}

fn compare_function(compare: wgpu::CompareFunction) -> &'static str {
    use wgpu::CompareFunction as C;
    match compare {
        C::Never => "never",
        C::Less => "less",
        C::Equal => "equal",
        C::LessEqual => "less-equal",
        C::Greater => "greater",
        C::NotEqual => "not-equal",
        C::GreaterEqual => "greater-equal",
        C::Always => "always",
    }
}

fn stencil_operation(operation: wgpu::StencilOperation) -> &'static str {
    use wgpu::StencilOperation as S;
    match operation {
        S::Keep => "keep",
        S::Zero => "zero",
        S::Replace => "replace",
        S::Invert => "invert",
        S::IncrementClamp => "increment-clamp",
        S::DecrementClamp => "decrement-clamp",
        S::IncrementWrap => "increment-wrap",
        S::DecrementWrap => "decrement-wrap",
    }
}

fn stencil_face(face: &wgpu::StencilFaceState) -> Object {
    object(&[
        ("compare", compare_function(face.compare).into()),
        ("depthFailOp", stencil_operation(face.depth_fail_op).into()),
        ("failOp", stencil_operation(face.fail_op).into()),
        ("passOp", stencil_operation(face.pass_op).into()),
    ])
}

fn depth_stencil(state: &wgpu::DepthStencilState) -> Result<Object, String> {
    let mapped = object(&[("format", texture_format(state.format)?.into())]);
    if let Some(compare) = state.depth_compare {
        set(&mapped, "depthCompare", compare_function(compare).into());
    }
    if let Some(write) = state.depth_write_enabled {
        set(&mapped, "depthWriteEnabled", write.into());
    }
    set(&mapped, "depthBias", state.bias.constant.into());
    set(&mapped, "depthBiasClamp", state.bias.clamp.into());
    set(
        &mapped,
        "depthBiasSlopeScale",
        state.bias.slope_scale.into(),
    );
    set(
        &mapped,
        "stencilBack",
        stencil_face(&state.stencil.back).into(),
    );
    set(
        &mapped,
        "stencilFront",
        stencil_face(&state.stencil.front).into(),
    );
    set(&mapped, "stencilReadMask", state.stencil.read_mask.into());
    set(&mapped, "stencilWriteMask", state.stencil.write_mask.into());
    Ok(mapped)
}

fn blend_factor(factor: wgpu::BlendFactor) -> Result<&'static str, String> {
    use wgpu::BlendFactor as B;
    Ok(match factor {
        B::Zero => "zero",
        B::One => "one",
        B::Src => "src",
        B::OneMinusSrc => "one-minus-src",
        B::SrcAlpha => "src-alpha",
        B::OneMinusSrcAlpha => "one-minus-src-alpha",
        B::Dst => "dst",
        B::OneMinusDst => "one-minus-dst",
        B::DstAlpha => "dst-alpha",
        B::OneMinusDstAlpha => "one-minus-dst-alpha",
        B::SrcAlphaSaturated => "src-alpha-saturated",
        B::Constant => "constant",
        B::OneMinusConstant => "one-minus-constant",
        other => return Err(format!("blend factor {other:?}")),
    })
}

fn blend_component(component: &wgpu::BlendComponent) -> Result<Object, String> {
    use wgpu::BlendOperation as O;
    let operation = match component.operation {
        O::Add => "add",
        O::Subtract => "subtract",
        O::ReverseSubtract => "reverse-subtract",
        O::Min => "min",
        O::Max => "max",
    };
    Ok(object(&[
        ("dstFactor", blend_factor(component.dst_factor)?.into()),
        ("operation", operation.into()),
        ("srcFactor", blend_factor(component.src_factor)?.into()),
    ]))
}

fn color_target(target: &wgpu::ColorTargetState) -> Result<Object, String> {
    let mapped = object(&[("format", texture_format(target.format)?.into())]);
    if let Some(blend) = &target.blend {
        let blend = object(&[
            ("alpha", blend_component(&blend.alpha)?.into()),
            ("color", blend_component(&blend.color)?.into()),
        ]);
        set(&mapped, "blend", blend.into());
    }
    set(&mapped, "writeMask", target.write_mask.bits().into());
    Ok(mapped)
}

fn primitive(state: &wgpu::PrimitiveState) -> Result<Object, String> {
    if state.polygon_mode != wgpu::PolygonMode::Fill || state.conservative {
        return Err("non-fill polygon mode".into());
    }
    let cull = match state.cull_mode {
        None => "none",
        Some(wgpu::Face::Front) => "front",
        Some(wgpu::Face::Back) => "back",
    };
    let front_face = match state.front_face {
        wgpu::FrontFace::Ccw => "ccw",
        wgpu::FrontFace::Cw => "cw",
    };
    let topology = match state.topology {
        wgpu::PrimitiveTopology::PointList => "point-list",
        wgpu::PrimitiveTopology::LineList => "line-list",
        wgpu::PrimitiveTopology::LineStrip => "line-strip",
        wgpu::PrimitiveTopology::TriangleList => "triangle-list",
        wgpu::PrimitiveTopology::TriangleStrip => "triangle-strip",
    };
    let mapped = object(&[
        ("cullMode", cull.into()),
        ("frontFace", front_face.into()),
        ("topology", topology.into()),
        ("unclippedDepth", state.unclipped_depth.into()),
    ]);
    if let Some(format) = state.strip_index_format {
        let format = match format {
            wgpu::IndexFormat::Uint16 => "uint16",
            wgpu::IndexFormat::Uint32 => "uint32",
        };
        set(&mapped, "stripIndexFormat", format.into());
    }
    Ok(mapped)
}
