// A worker with a GPU device of its own that compiles the page's render pipelines and
// draws with each once, for Safari (`crates/render/src/gpu/webgpu/precompile/workers.rs`
// says why). Safari keeps what any device compiled for its whole GPU process, so the
// page's device then creates the same pipeline in a few milliseconds and its first draw
// does not stall. The page starts it (`sloppyPipelineWorker` in `index.html`).
//
// A pipeline arrives as plain data: the browser descriptor the page would pass, with
// each stage's WGSL in place of its module and the bind group layouts' entries in place
// of the layout. The draw needs the pipeline's own targets (formats, sample count) and
// every bind group and vertex buffer set, but no real data: a draw from zero-filled
// buffers that covers no pixel compiles what the page's first draw would.

interface DeviceWish {
  features: GPUFeatureName[];
  limits: Record<string, number>;
}

/** A pipeline descriptor with texts and layout entries in place of GPU objects. */
interface PlainPipeline {
  label: string;
  layout: GPUBindGroupLayoutEntry[][];
  vertex: Omit<GPUVertexState, "module"> & { module: string };
  fragment?: Omit<GPUFragmentState, "module"> & { module: string };
  primitive?: GPUPrimitiveState;
  depthStencil?: GPUDepthStencilState;
  multisample?: GPUMultisampleState;
}

type Request = { open: DeviceWish } | { job: number; pipeline: PlainPipeline } | { close: true };

/** A job's answer: `error` when this worker could not compile it, and the page will. */
export type JobReply = { job: number; error?: string };
/** This worker can compile nothing more: no device, or it was lost. */
export type BrokenReply = { broken: string };

// GPUBufferUsage and GPUTextureUsage bits (the type library lacks the constants).
const BUFFER_VERTEX = 0x20;
const BUFFER_UNIFORM = 0x40;
const BUFFER_STORAGE = 0x80;
const TEXTURE_BINDING = 0x04;
const RENDER_ATTACHMENT = 0x10;

/** Every buffer binding's and vertex buffer's size: the largest uniform binding WebGPU
 * allows by default, which holds any of the game's blocks and a draw's vertices. */
const BUFFER_BYTES = 65536;
/** Edge of the placeholder textures and targets. */
const SIZE = 4;

let device: Promise<GPUDevice> | undefined;
let lost = false;
const layouts = new Map<string, { groups: GPUBindGroupLayout[]; pipeline: GPUPipelineLayout }>();
const placeholders = new Map<string, GPUBuffer | GPUTextureView | GPUSampler>();
const targets = new Map<string, { colors: (GPUTextureView | null)[]; depth?: GPUTextureView }>();

async function open(wish: DeviceWish) {
  const adapter = await navigator.gpu?.requestAdapter({ powerPreference: "high-performance" });
  if (!adapter) throw new Error("no WebGPU adapter in a worker");
  const opened = await adapter.requestDevice({
    requiredFeatures: wish.features,
    requiredLimits: wish.limits,
  });
  opened.lost.then((info) => {
    lost = true;
    if (info.reason !== "destroyed") postMessage({ broken: `device lost: ${info.message}` });
  });
  return opened;
}

function layoutFor(gpu: GPUDevice, groups: GPUBindGroupLayoutEntry[][]) {
  const key = JSON.stringify(groups);
  let layout = layouts.get(key);
  if (!layout) {
    const bindGroups = groups.map((entries) => gpu.createBindGroupLayout({ entries }));
    layout = {
      groups: bindGroups,
      pipeline: gpu.createPipelineLayout({ bindGroupLayouts: bindGroups }),
    };
    layouts.set(key, layout);
  }
  return layout;
}

function placeholder<T extends GPUBuffer | GPUTextureView | GPUSampler>(
  key: string,
  make: () => T,
) {
  let held = placeholders.get(key) as T | undefined;
  if (!held) {
    held = make();
    placeholders.set(key, held);
  }
  return held;
}

const sharedBuffer = (gpu: GPUDevice) =>
  placeholder("buffer", () =>
    gpu.createBuffer({
      size: BUFFER_BYTES,
      usage: BUFFER_UNIFORM | BUFFER_STORAGE | BUFFER_VERTEX,
    }),
  );

const TEXTURE_FORMATS: Record<GPUTextureSampleType, GPUTextureFormat> = {
  float: "rgba8unorm",
  "unfilterable-float": "rgba8unorm",
  depth: "depth32float",
  uint: "r32uint",
  sint: "r32sint",
};

function resource(gpu: GPUDevice, entry: GPUBindGroupLayoutEntry): GPUBindingResource {
  if (entry.buffer) {
    // A writable one gets a buffer of its own: one buffer may not be bound for writing
    // and reading in a draw.
    if (entry.buffer.type === "storage") {
      return {
        buffer: placeholder("writable buffer", () =>
          gpu.createBuffer({ size: BUFFER_BYTES, usage: BUFFER_STORAGE }),
        ),
      };
    }
    return { buffer: sharedBuffer(gpu) };
  }
  if (entry.sampler) {
    const type = entry.sampler.type ?? "filtering";
    return placeholder(`sampler ${type}`, () =>
      gpu.createSampler(
        type === "comparison"
          ? { compare: "less" }
          : type === "filtering"
            ? { magFilter: "linear", minFilter: "linear" }
            : {},
      ),
    );
  }
  if (entry.texture) {
    const format = TEXTURE_FORMATS[entry.texture.sampleType ?? "float"];
    const multisampled = entry.texture.multisampled ?? false;
    if ((entry.texture.viewDimension ?? "2d") !== "2d") {
      throw new Error(`a ${entry.texture.viewDimension} texture binding`);
    }
    return placeholder(`texture ${format} ${multisampled}`, () =>
      gpu
        .createTexture({
          size: [SIZE, SIZE],
          format,
          sampleCount: multisampled ? 4 : 1,
          usage: TEXTURE_BINDING | (multisampled ? RENDER_ATTACHMENT : 0),
        })
        .createView(),
    );
  }
  throw new Error(`binding ${entry.binding} of a kind the worker does not make`);
}

/** Attachments of the pipeline's own formats and sample count. */
function attachments(gpu: GPUDevice, pipeline: PlainPipeline) {
  const samples = pipeline.multisample?.count ?? 1;
  const colors = [...(pipeline.fragment?.targets ?? [])].map((target) => target?.format ?? null);
  const depth = pipeline.depthStencil?.format;
  const key = JSON.stringify([samples, colors, depth]);
  let views = targets.get(key);
  if (!views) {
    const make = (format: GPUTextureFormat) =>
      gpu
        .createTexture({
          size: [SIZE, SIZE],
          format,
          sampleCount: samples,
          usage: RENDER_ATTACHMENT,
        })
        .createView();
    views = {
      colors: colors.map((format) => format && make(format)),
      depth: depth && make(depth),
    };
    targets.set(key, views);
  }
  return views;
}

async function compileAndDraw(gpu: GPUDevice, plain: PlainPipeline) {
  const layout = layoutFor(gpu, plain.layout);
  const module = (stage: string, code: string) =>
    gpu.createShaderModule({ label: `${plain.label} ${stage}`, code });
  const descriptor: GPURenderPipelineDescriptor = {
    ...plain,
    layout: layout.pipeline,
    vertex: { ...plain.vertex, module: module("vertex", plain.vertex.module) },
    fragment: plain.fragment && {
      ...plain.fragment,
      module: module("fragment", plain.fragment.module),
    },
  };
  gpu.pushErrorScope("validation");
  let failure: unknown;
  try {
    const pipeline = await gpu.createRenderPipelineAsync(descriptor);
    const views = attachments(gpu, plain);
    const encoder = gpu.createCommandEncoder();
    const pass = encoder.beginRenderPass({
      colorAttachments: views.colors.map(
        (view) => view && { view, loadOp: "clear", storeOp: "store" },
      ),
      depthStencilAttachment: views.depth && {
        view: views.depth,
        depthClearValue: 1,
        depthLoadOp: "clear",
        depthStoreOp: "store",
      },
    });
    pass.setPipeline(pipeline);
    layout.groups.forEach((group, index) => {
      const entries = plain.layout[index].map((entry) => ({
        binding: entry.binding,
        resource: resource(gpu, entry),
      }));
      pass.setBindGroup(index, gpu.createBindGroup({ layout: group, entries }));
    });
    [...(plain.vertex.buffers ?? [])].forEach((_, slot) =>
      pass.setVertexBuffer(slot, sharedBuffer(gpu)),
    );
    pass.draw(3);
    pass.end();
    gpu.queue.submit([encoder.finish()]);
    await gpu.queue.onSubmittedWorkDone();
  } catch (error) {
    failure = error;
  }
  const invalid = await gpu.popErrorScope();
  if (failure) throw failure;
  if (invalid) throw new Error(invalid.message);
}

onmessage = async ({ data }: MessageEvent<Request>) => {
  if ("open" in data) {
    device = open(data.open);
    device.catch((error) => postMessage({ broken: String(error?.message ?? error) }));
    return;
  }
  if ("close" in data) {
    (await device?.catch(() => undefined))?.destroy();
    close();
    return;
  }
  const reply: JobReply = { job: data.job };
  try {
    const gpu = await device;
    if (!gpu || lost) throw new Error("no device");
    await compileAndDraw(gpu, data.pipeline);
  } catch (error) {
    reply.error = String((error as Error)?.message ?? error);
  }
  postMessage(reply);
};
