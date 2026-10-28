//! The browser's background compile: `createRenderPipelineAsync` for the descriptor
//! wgpu's WebGPU backend would build from a [`PipelineSpec`] (the parent module says
//! why).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::rc::Rc;

use js_sys::{Array, Object, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use super::{LayoutKind, PipelineSpec};
use crate::shader::StageSources;

/// A pipeline the browser compiled in the background, for wgpu to wrap (null for one
/// that failed).
pub type Compiled = JsValue;

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

/// What the device's `createShaderModule` answers wgpu while it wraps a compiled
/// pipeline ([`hand_over`]); no pipeline uses it.
const PLACEHOLDER_WGSL: &str =
    "@vertex fn placeholder() -> @builtin(position) vec4f { return vec4f(0.0); }";

thread_local! {
    /// The placeholder module, made once.
    static PLACEHOLDER: std::cell::OnceCell<JsValue> = const { std::cell::OnceCell::new() };
}

/// Run `create`, wgpu creating a pipeline, with the browser device's
/// `createRenderPipeline` answering `pipeline` (compiled in the background from the
/// same spec) and its `createShaderModule` a placeholder, so wgpu wraps that pipeline
/// rather than having the browser build its twin. While set, the device's own
/// properties shadow its prototype's methods, which wasm-bindgen calls as methods.
pub(super) fn hand_over<T>(
    device: &wgpu::Device,
    pipeline: &JsValue,
    create: impl FnOnce() -> T,
) -> T {
    let raw = raw_device(device);
    let module = PLACEHOLDER.with(|placeholder| {
        placeholder
            .get_or_init(|| raw.create_shader_module(&object(&[("code", PLACEHOLDER_WGSL.into())])))
            .clone()
    });
    let pipeline = pipeline.clone();
    let answer_pipeline = Closure::<dyn Fn() -> JsValue>::new(move || pipeline.clone());
    let answer_module = Closure::<dyn Fn() -> JsValue>::new(move || module.clone());
    let target: &Object = raw.unchecked_ref();
    set(
        target,
        "createRenderPipeline",
        answer_pipeline.as_ref().clone(),
    );
    set(target, "createShaderModule", answer_module.as_ref().clone());
    let created = create();
    for name in ["createRenderPipeline", "createShaderModule"] {
        Reflect::delete_property(target, &name.into()).expect("the device's own property");
    }
    created
}

/// Browser shader modules made from the same WGSL as wgpu's: the vertex stage's and,
/// if any, the fragment stage's.
struct RawModules {
    vertex: JsValue,
    fragment: Option<JsValue>,
}

/// A fixed set of pipelines compiling in the background from the moment the
/// renderer exists, so the page builds the arena meanwhile; the owner creates the
/// wgpu pipelines once [`Background::done`] (or at once when it cannot wait).
pub struct Background {
    /// The compiled pipelines in job order once all have finished (null for one that
    /// failed), for wgpu to wrap.
    compiled: Rc<RefCell<Option<Vec<JsValue>>>>,
}

impl Background {
    pub fn start(device: &wgpu::Device, jobs: &[(&PipelineSpec, &StageSources)]) -> Self {
        let compiled = Rc::new(RefCell::new(None));
        let held = compiled.clone();
        // Issued now: a spawned task would only start once the page yields, after the
        // arena these compiles should overlap.
        let compiling = Precompiler::<()>::new(device).compile_all(jobs);
        wasm_bindgen_futures::spawn_local(async move {
            *held.borrow_mut() = Some(compiling.await);
        });
        Self { compiled }
    }

    pub fn done(&self) -> bool {
        self.compiled.borrow().is_some()
    }

    /// The compiled pipeline of job `index`, once all have finished.
    pub fn compiled(&self, index: usize) -> Option<JsValue> {
        self.compiled.borrow().as_ref()?.get(index).cloned()
    }
}

/// The browser `GPUDevice` behind wgpu's.
fn raw_device(device: &wgpu::Device) -> RawDevice {
    device
        .as_webgpu()
        .expect("the renderer runs on the browser's WebGPU")
        .unchecked_ref::<RawDevice>()
        .clone()
}

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
        Self {
            device: raw_device(device),
            layouts: RefCell::default(),
            queue: Rc::new(RefCell::new(Queue {
                started: HashSet::new(),
                waiting: VecDeque::new(),
                running: 0,
                finished: HashMap::new(),
            })),
        }
    }

    /// Browser modules for one pipeline (`pipelines.rs` says why not shared).
    fn modules(&self, label: &str, sources: &StageSources) -> RawModules {
        let module = |stage, source: &str| {
            let label = format!("{label} {stage}");
            let descriptor = object(&[("label", label.into()), ("code", source.into())]);
            self.device.create_shader_module(&descriptor)
        };
        RawModules {
            vertex: module("vertex", &sources.vertex),
            fragment: sources
                .fragment
                .as_deref()
                .map(|source| module("fragment", source)),
        }
    }

    /// Queue `key` for compiling; callers start only keys not [`queued`](Self::queued)
    /// yet.
    pub fn start(&self, key: K, spec: &PipelineSpec, sources: &StageSources) {
        self.queue.borrow_mut().started.insert(key);
        match self.descriptor(spec, &self.modules(spec.label, sources)) {
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

    /// The compiled pipeline of `key` (null if it failed), for wgpu to wrap; the
    /// precompiler lets go of it.
    pub fn take(&self, key: &K) -> Option<JsValue> {
        self.queue.borrow_mut().finished.remove(key)
    }

    /// Pipelines waiting for or being compiled.
    pub fn compiling(&self) -> u32 {
        let queue = self.queue.borrow();
        (queue.waiting.len() + queue.running) as u32
    }

    fn pump(&self) {
        let device = &self.device;
        let mut queue = self.queue.borrow_mut();
        while queue.running < CONCURRENCY && queue.running < queue.waiting.len() {
            queue.running += 1;
            let (shared, device) = (self.queue.clone(), device.clone());
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

    /// Start compiling every spec in the background now; the future resolves to the
    /// compiled pipelines in job order (null for one that could not compile), for the
    /// few fixed ones ([`Background`]).
    pub fn compile_all(
        &self,
        jobs: &[(&PipelineSpec, &StageSources)],
    ) -> impl Future<Output = Vec<JsValue>> + 'static {
        let mut promises = Vec::with_capacity(jobs.len());
        for (spec, sources) in jobs {
            match self.descriptor(spec, &self.modules(spec.label, sources)) {
                Ok(descriptor) => {
                    promises.push(self.device.create_render_pipeline_async(&descriptor))
                }
                Err(error) => {
                    web_sys::console::warn_1(
                        &format!("Cannot precompile the {} pipeline: {error}", spec.label).into(),
                    );
                    promises.push(Promise::resolve(&JsValue::NULL));
                }
            }
        }
        async move {
            let mut compiled = Vec::with_capacity(promises.len());
            for promise in promises {
                compiled.push(JsFuture::from(promise).await.unwrap_or_else(|error| {
                    web_sys::console::warn_2(&"Pipeline precompile failed:".into(), &error);
                    JsValue::NULL
                }));
            }
            compiled
        }
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
    fn descriptor(&self, spec: &PipelineSpec, modules: &RawModules) -> Result<Object, String> {
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
            ("module", modules.vertex.clone()),
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
            let module = modules
                .fragment
                .clone()
                .ok_or("a fragment entry point without a fragment source")?;
            let fragment = object(&[
                ("module", module),
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
            let view_dimension = match view_dimension {
                wgpu::TextureViewDimension::D2 => "2d",
                other => return Err(format!("view dimension {other:?}")),
            };
            let texture = object(&[
                ("multisampled", multisampled.into()),
                ("sampleType", sample_type.into()),
                ("viewDimension", view_dimension.into()),
            ]);
            set(&mapped, "texture", texture.into());
        }
        other => return Err(format!("binding type {other:?}")),
    }
    Ok(mapped.into())
}

fn vertex_format(format: wgpu::VertexFormat) -> Result<&'static str, String> {
    use wgpu::VertexFormat as F;
    Ok(match format {
        F::Float32x2 => "float32x2",
        F::Float32x3 => "float32x3",
        F::Float32x4 => "float32x4",
        F::Uint32 => "uint32",
        other => return Err(format!("vertex format {other:?}")),
    })
}

fn texture_format(format: wgpu::TextureFormat) -> Result<&'static str, String> {
    use wgpu::TextureFormat as F;
    Ok(match format {
        F::Rgba8Unorm => "rgba8unorm",
        F::Rgba8UnormSrgb => "rgba8unorm-srgb",
        F::Bgra8Unorm => "bgra8unorm",
        F::Rgba16Float => "rgba16float",
        F::Rg11b10Ufloat => "rg11b10ufloat",
        F::R8Unorm => "r8unorm",
        F::Depth32Float => "depth32float",
        F::Depth16Unorm => "depth16unorm",
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
        B::SrcAlpha => "src-alpha",
        B::OneMinusSrcAlpha => "one-minus-src-alpha",
        other => return Err(format!("blend factor {other:?}")),
    })
}

fn blend_component(component: &wgpu::BlendComponent) -> Result<Object, String> {
    if component.operation != wgpu::BlendOperation::Add {
        return Err(format!("blend operation {:?}", component.operation));
    }
    Ok(object(&[
        ("dstFactor", blend_factor(component.dst_factor)?.into()),
        ("operation", "add".into()),
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
    if state.topology != wgpu::PrimitiveTopology::TriangleList
        || state.strip_index_format.is_some()
        || state.polygon_mode != wgpu::PolygonMode::Fill
        || state.conservative
    {
        return Err("primitives other than filled triangle lists".into());
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
    Ok(object(&[
        ("cullMode", cull.into()),
        ("frontFace", front_face.into()),
        ("topology", "triangle-list".into()),
        ("unclippedDepth", state.unclipped_depth.into()),
    ]))
}
