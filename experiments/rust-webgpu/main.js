import "./style.css";
import init, { Lab, physics_probe } from "@rust-lab/lab.js";
import wasmUrl from "@rust-lab/lab_bg.wasm?url";

const canvas = document.querySelector("#scene");
const loading = document.querySelector("#loading");
const status = document.querySelector("#status");
const state = document.querySelector("#state");
const launch = document.querySelector("#launch");
const pause = document.querySelector("#pause");
const reset = document.querySelector("#reset");
const bodies = document.querySelector("#bodies");
const awake = document.querySelector("#awake");
const fps = document.querySelector("#fps");
const events = new AbortController();
let lab;
let paused = false;
let previous;
let frameId;
let dragging = false;
let frames = 0;
let meterStart = 0;
let disposed = false;

function fail(error) {
  cancelAnimationFrame(frameId);
  document.body.dataset.state = "error";
  state.textContent = "UNAVAILABLE";
  loading.hidden = false;
  loading.querySelector("h2").textContent = "The yard couldn't start";
  status.textContent = String(error);
  for (const button of [launch, pause, reset]) button.disabled = true;
  console.error(error);
}

function resetScene() {
  lab.reset();
  previous = undefined;
  updateStats();
}
function updateStats() {
  bodies.textContent = lab.body_count();
  awake.textContent = lab.active_count();
}
function resize() {
  const bounds = canvas.getBoundingClientRect();
  const scale = Math.min(devicePixelRatio, 2);
  const width = Math.max(1, Math.round(bounds.width * scale));
  const height = Math.max(1, Math.round(bounds.height * scale));
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
    lab.resize(width, height);
  }
}
function frame(now) {
  try {
    const dt = previous === undefined ? 0 : (now - previous) / 1000;
    previous = now;
    lab.frame(dt, paused || document.hidden);
    frames++;
    if (now - meterStart >= 500) {
      fps.textContent = Math.round((frames * 1000) / (now - meterStart));
      frames = 0;
      meterStart = now;
      updateStats();
    }
    frameId = requestAnimationFrame(frame);
  } catch (error) {
    fail(error);
  }
}
const observer = new ResizeObserver(() => {
  if (lab) resize();
});
function dispose() {
  if (disposed) return;
  disposed = true;
  cancelAnimationFrame(frameId);
  observer.disconnect();
  events.abort();
  lab?.free();
  lab = undefined;
  delete window.rustLab;
}

async function start() {
  if (!navigator.gpu)
    throw new Error(
      "WebGPU is required. Open this local experiment in a browser with WebGPU enabled.",
    );
  await init({ module_or_path: wasmUrl });
  status.textContent = "Preparing WebGPU and the physics world…";
  const featureChecks = Array.from(physics_probe(1));
  if (featureChecks.length !== 11 || featureChecks.some((n) => n === 0)) {
    throw new Error(`Rapier feature probe failed: ${featureChecks}`);
  }
  document.querySelector(".footnote").textContent =
    `Rapier feature probe: ${featureChecks.length}/${featureChecks.length} passed.`;
  lab = await Lab.create(canvas);
  if (disposed) {
    lab.free();
    lab = undefined;
    return;
  }
  resize();
  observer.observe(canvas);
  const on = (target, name, handler, options = {}) =>
    target.addEventListener(name, handler, { ...options, signal: events.signal });
  on(launch, "click", () => {
    lab.launch();
    updateStats();
  });
  on(reset, "click", resetScene);
  on(pause, "click", () => {
    paused = !paused;
    pause.textContent = paused ? "Resume" : "Pause";
    state.textContent = paused ? "PAUSED" : "SIMULATION LIVE";
    pause.setAttribute("aria-pressed", String(paused));
  });
  on(canvas, "pointerdown", (event) => {
    if (event.button !== 0) return;
    dragging = true;
    canvas.setPointerCapture(event.pointerId);
    canvas.focus();
  });
  on(canvas, "pointermove", (event) => {
    if (dragging) lab.orbit(event.movementX, event.movementY);
  });
  for (const name of ["pointerup", "pointercancel", "lostpointercapture"])
    on(canvas, name, () => {
      dragging = false;
    });
  on(
    canvas,
    "wheel",
    (event) => {
      event.preventDefault();
      lab.zoom(event.deltaY);
    },
    { passive: false },
  );
  on(window, "keydown", (event) => {
    if (event.repeat || event.target.closest("button, input, textarea, select, a")) return;
    if (event.code === "Space") {
      event.preventDefault();
      lab.launch();
      updateStats();
    }
    if (event.code === "KeyR") resetScene();
  });
  on(document, "visibilitychange", () => {
    previous = undefined;
  });
  // A read-only probe for the standalone smoke check and browser inspection.
  window.rustLab = {
    featureChecks,
    snapshot: () => ({
      bodies: lab.body_count(),
      awake: lab.active_count(),
      ticks: lab.ticks(),
      displacement: lab.stack_displacement(),
      paused,
    }),
  };
  lab.frame(0, false);
  updateStats();
  for (const button of [launch, pause, reset]) button.disabled = false;
  loading.hidden = true;
  document.body.dataset.state = "ready";
  state.textContent = "SIMULATION LIVE";
  meterStart = performance.now();
  frameId = requestAnimationFrame(frame);
}
window.addEventListener("error", (event) => fail(event.error ?? event.message), {
  signal: events.signal,
});
window.addEventListener("unhandledrejection", (event) => fail(event.reason), {
  signal: events.signal,
});
window.addEventListener("pagehide", dispose, { once: true });
window.addEventListener("pageshow", (event) => {
  if (event.persisted) location.reload();
});
if (import.meta.hot) import.meta.hot.dispose(dispose);
start().catch(fail);
