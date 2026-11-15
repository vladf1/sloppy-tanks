// The page's opt-in trace, for devices with no inspector at hand. `index.html` loads
// this script only in a browser where `?trace` turned it on, before anything it watches
// starts. Each second, and at every GPU, network or page event, it notes what the page
// does in `localStorage`, which outlives a page the system killed, and uploads the notes
// to the directory's `/traces`.
const NOTES = "sloppy-trace-notes";
const MAX_NOTES = 600;
// A keepalive request carries at most 64 KB; a dump of GPU calls takes at most
// half of an upload, so the notes around it (the loss itself) go up with it.
const MAX_UPLOAD = 60000;
const MAX_DUMP = 30000;
const TRACES = "https://directory.sloppytanks.com/traces";
const id = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
let notes = [];
try {
  notes = JSON.parse(localStorage.getItem(NOTES) ?? "[]");
} catch {
  // No earlier notes that can be read: this load starts its own.
}
const started = performance.timeOrigin;
const gpu = { devices: 0, buffers: 0, textures: 0, peak: 0, lost: [] };
let wasm;
const note = (kind, detail = "") => {
  const at = new Date().toISOString().slice(11, 23);
  notes.push(`${at} ${kind} ${kind === "gpu-ops" ? detail : String(detail).slice(0, 600)}`);
  if (notes.length > MAX_NOTES) notes.splice(0, notes.length - MAX_NOTES);
};
const save = () => {
  try {
    localStorage.setItem(NOTES, JSON.stringify(notes));
  } catch {
    // Storage is full or off: the notes still go up from memory.
  }
};
/** The latest whole notes that fit an upload, with this load's own first one,
 * which names the page, the screen and the browser. */
const uploadBody = () => {
  const latest = [];
  let size = 0;
  for (let index = notes.length - 1; index >= 0; index--) {
    size += notes[index].length + 1;
    if (size > MAX_UPLOAD) break;
    latest.push(notes[index]);
  }
  latest.reverse();
  return (latest.includes(loaded) ? latest : [loaded, ...latest]).join("\n");
};
const upload = () =>
  fetch(`${TRACES}?id=${id}`, {
    method: "POST",
    body: uploadBody(),
    keepalive: true,
  }).catch(() => {});
const mb = (bytes) => (bytes / 1e6).toFixed(1);
note(
  "load",
  `${location.href} ${innerWidth}x${innerHeight}@${devicePixelRatio} ${navigator.userAgent}`,
);
const loaded = notes[notes.length - 1];
// The previous load's notes go up first: it may have ended without a word.
upload();

for (const name of ["instantiateStreaming", "instantiate"]) {
  const original = WebAssembly[name];
  WebAssembly[name] = async (...args) => {
    const result = await original.apply(WebAssembly, args);
    const instance = result.instance ?? result;
    for (const value of Object.values(instance.exports ?? {})) {
      if (value instanceof WebAssembly.Memory) wasm = value;
    }
    return result;
  };
}
const bytesPerTexel = (format) =>
  /32float|32uint|32sint/.test(format) && /rgba/.test(format)
    ? 16
    : /rgba16|rg32|depth32float-stencil8/.test(format)
      ? 8
      : /^r8|stencil8$/.test(format)
        ? 1
        : /^rg8|r16|depth16/.test(format)
          ? 2
          : 4;
if (globalThis.GPUAdapter) {
  const requestDevice = GPUAdapter.prototype.requestDevice;
  GPUAdapter.prototype.requestDevice = async function (...options) {
    const device = await requestDevice.apply(this, options);
    const number = ++gpu.devices;
    note("gpu-device", `#${number} ${JSON.stringify(this.info ?? {})}`);
    device.lost.then((info) => {
      gpu.lost.push(number);
      note("gpu-lost", `#${number} ${info.reason} "${info.message}"`);
      note(
        "gpu-ops",
        `the last GPU calls before the loss:\n${operations.join("\n").slice(-MAX_DUMP)}`,
      );
      save();
      upload();
    });
    device.addEventListener("uncapturederror", (event) =>
      note("gpu-error", `#${number} ${event.error?.constructor?.name} ${event.error?.message}`),
    );
    const track = (kind, bytes, object, label) => {
      gpu[kind] += bytes;
      gpu.peak = Math.max(gpu.peak, gpu.buffers + gpu.textures);
      if (bytes >= 8e6) note(`gpu-${kind}`, `#${number} ${label} ${mb(bytes)} MB`);
      const destroy = object.destroy.bind(object);
      let live = true;
      object.destroy = () => {
        if (live) gpu[kind] -= bytes;
        live = false;
        destroy();
      };
      return object;
    };
    const createBuffer = device.createBuffer.bind(device);
    device.createBuffer = (d) => track("buffers", d.size, createBuffer(d), d.label);
    const createTexture = device.createTexture.bind(device);
    device.createTexture = (d) => {
      const [w, h = 1, l = 1] = Array.isArray(d.size)
        ? d.size
        : [d.size.width, d.size.height, d.size.depthOrArrayLayers];
      let texels = 0;
      for (let m = 0; m < (d.mipLevelCount ?? 1); m++) {
        texels += Math.max(1, w >> m) * Math.max(1, h >> m) * l;
      }
      const bytes = texels * bytesPerTexel(d.format) * (d.sampleCount ?? 1);
      const label = `${d.label} ${w}x${h}x${l} ${d.format} x${d.sampleCount ?? 1}`;
      return track("textures", bytes, createTexture(d), label);
    };
    const destroyDevice = device.destroy.bind(device);
    device.destroy = () => {
      note("gpu-destroy", `#${number}`);
      destroyDevice();
    };
    return device;
  };
}
// The GPU work last sent, for a device the GPU process lost: each call by name,
// with what it names (labels, sizes, counts).
const operations = [];
const MAX_OPERATIONS = 1200;
const name = (object) =>
  object
    ? `${object.label || object.constructor?.name}${object.size !== undefined ? `[${object.size}]` : ""}${object.width ? `[${object.width}x${object.height} ${object.format}]` : ""}`
    : String(object);
const describe = (value) =>
  value && typeof value === "object"
    ? ArrayBuffer.isView(value) || value instanceof ArrayBuffer
      ? `<${value.byteLength}B>`
      : value.label !== undefined || value.size !== undefined
        ? name(value)
        : Array.isArray(value)
          ? `[${value.length}]`
          : JSON.stringify(value, (key, inner) =>
              inner &&
              typeof inner === "object" &&
              (inner.label !== undefined || inner.size !== undefined) &&
              key
                ? name(inner)
                : inner,
            )?.slice(0, 300)
    : String(value);
const remember = (operation) => {
  operations.push(operation);
  if (operations.length > MAX_OPERATIONS) {
    operations.splice(0, operations.length - MAX_OPERATIONS);
  }
};
const record = (type, method) => {
  const prototype = globalThis[type]?.prototype;
  const original = prototype?.[method];
  if (!original) return;
  prototype[method] = function (...args) {
    remember(
      `${type.replace("GPU", "")}.${method}(${args.map(describe).join(", ")}) on ${name(this)}`,
    );
    return original.apply(this, args);
  };
};
for (const method of ["submit", "writeBuffer", "writeTexture", "copyExternalImageToTexture"]) {
  record("GPUQueue", method);
}
for (const method of [
  "beginRenderPass",
  "beginComputePass",
  "copyBufferToBuffer",
  "copyBufferToTexture",
  "copyTextureToTexture",
  "copyTextureToBuffer",
  "clearBuffer",
  "resolveQuerySet",
  "finish",
]) {
  record("GPUCommandEncoder", method);
}
for (const method of [
  "setPipeline",
  "setBindGroup",
  "setVertexBuffer",
  "setIndexBuffer",
  "draw",
  "drawIndexed",
  "drawIndirect",
  "drawIndexedIndirect",
  "setViewport",
  "setScissorRect",
  "executeBundles",
  "end",
]) {
  record("GPURenderPassEncoder", method);
}
for (const method of ["setPipeline", "setBindGroup", "dispatchWorkgroups", "end"]) {
  record("GPUComputePassEncoder", method);
}
for (const method of ["configure", "getCurrentTexture", "unconfigure"]) {
  record("GPUCanvasContext", method);
}
for (const method of [
  "createRenderPipelineAsync",
  "createRenderPipeline",
  "createComputePipeline",
  "createBindGroup",
]) {
  record("GPUDevice", method);
}
for (const method of ["destroy", "mapAsync", "unmap"]) {
  record("GPUBuffer", method);
}
record("GPUTexture", "destroy");
const animationFrame = requestAnimationFrame;
window.requestAnimationFrame = (callback) =>
  animationFrame((time) => {
    remember(`--- frame ${time.toFixed(1)}`);
    return callback(time);
  });
const NativeSocket = WebSocket;
window.WebSocket = class extends NativeSocket {
  constructor(...args) {
    super(...args);
    const url = String(args[0]).replace(/\?.*/, "");
    note("socket", `open ${url}`);
    this.addEventListener("open", () => note("socket", `opened ${url}`));
    this.addEventListener("close", (event) =>
      note("socket", `closed ${url} ${event.code} ${event.reason}`),
    );
    this.addEventListener("error", () => note("socket", `error ${url}`));
  }
};
const consoleError = console.error.bind(console);
console.error = (...args) => {
  note("console-error", args.map(String).join(" "));
  save();
  consoleError(...args);
};
addEventListener("error", (event) => note("error", `${event.message} ${event.error?.stack ?? ""}`));
addEventListener("unhandledrejection", (event) =>
  note("rejection", `${event.reason?.message ?? event.reason} ${event.reason?.stack ?? ""}`),
);
addEventListener("sloppy:failed", (event) => {
  note("failed", event.detail);
  save();
  upload();
});
addEventListener("visibilitychange", () => note("visibility", document.visibilityState));
addEventListener("pagehide", () => {
  // A page that unloads itself leaves no GPU calls for the next load; one the
  // back-forward cache keeps stores them again when it is shown.
  clearInterval(storeOperations);
  try {
    localStorage.removeItem(OPERATIONS);
  } catch {
    // Storage is off, so nothing was stored.
  }
  note("pagehide");
  save();
  upload();
});
new PerformanceObserver((list) => {
  for (const entry of list.getEntries()) {
    if (entry.name.startsWith("sloppy:")) note("mark", entry.name);
  }
}).observe({ type: "mark", buffered: true });
let seconds = 0;
setInterval(() => {
  const canvas = document.querySelector("canvas");
  note(
    "tick",
    [
      `t=${Math.round((Date.now() - started) / 1000)}s`,
      `shell=${document.documentElement.dataset.shell ?? "-"}`,
      `wasm=${wasm ? mb(wasm.buffer.byteLength) : "-"}MB`,
      `gpu=${mb(gpu.buffers)}+${mb(gpu.textures)}MB peak=${mb(gpu.peak)}`,
      `devices=${gpu.devices} lost=${gpu.lost.join(",") || "-"}`,
      `canvas=${canvas ? `${canvas.width}x${canvas.height}` : "-"}`,
      document.visibilityState,
    ].join(" "),
  );
  save();
  if (++seconds % 5 === 0) upload();
}, 1000);
// A page the system kills says nothing more: its latest GPU calls go to storage
// each second, and the next load uploads them.
const OPERATIONS = "sloppy-trace-operations";
const STORED_OPERATIONS = 300;
try {
  const earlier = localStorage.getItem(OPERATIONS);
  if (earlier) {
    notes.splice(
      notes.length - 1,
      0,
      `-- the previous load's last GPU calls:\n${earlier.slice(-MAX_DUMP)}`,
    );
    localStorage.removeItem(OPERATIONS);
    upload();
  }
} catch {
  // Storage is off, so no earlier load left its calls.
}
const storeOperationsEachSecond = () =>
  setInterval(() => {
    try {
      localStorage.setItem(OPERATIONS, operations.slice(-STORED_OPERATIONS).join("\n"));
    } catch {
      // Storage is full or off: a killed page then leaves no calls behind.
    }
  }, 1000);
let storeOperations = storeOperationsEachSecond();
addEventListener("pageshow", (event) => {
  if (event.persisted) {
    note("pageshow", "from the back-forward cache");
    storeOperations = storeOperationsEachSecond();
  }
});
