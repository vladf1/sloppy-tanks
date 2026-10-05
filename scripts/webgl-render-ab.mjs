// Matched A/B render-CPU measurement of WebGL engines served by dev servers.
// TEMPORARY benchmark for the direct WebGL2 backend work (see
// docs/webgl-direct-backend-plan.md): move it to the ignored artifacts/performance/
// or delete it before that PR merges. Usage, with one dev server per build:
//   node scripts/webgl-render-ab.mjs <out.json> <label=url> [<label=url> ...]
//   e.g. baseline=http://127.0.0.1:5174/sloppy-tanks/ direct=http://127.0.0.1:5173/sloppy-tanks/
//
// Frames are stepped, not paced by the GPU: the game's `loop` animation callback is
// held (as `freezeLoop` does) and run at fixed 1/60 s timestamps, so every page draws
// the same simulation states from the same overview camera. Before each frame the
// harness waits for the GPU process to finish the previous one (`gl.finish()` after a
// compositor frame), outside the timed region. With AB_PIN=1 (Linux only) the page's
// main thread is pinned to core 0 and every other browser thread to the rest, which
// matters where software GL (SwiftShader) competes for the same cores.
// The engine's own `renderMs` (performance.now() around View::render) is the sample;
// Chrome coarsens performance.now() to 0.1 ms, so compare means over many frames.
//
// Env: AB_SCENES (village,harbor,stress-test), AB_ROUNDS (ABBA repeats), AB_WARMUP
// (frames), AB_FRAMES (timed frames), AB_WIDTH, AB_HEIGHT, AB_SEED, AB_COUNT_CALLS=1
// (count WebGL calls per frame; the counting wrapper slows the page, so do not trust
// that run's timings), AB_PROFILE=<dir> (CPU profile of the timed frames; build the
// engine with CARGO_PROFILE_RELEASE_STRIP=false to keep Wasm function names),
// AB_CAMERA (overview|follow), AB_PIN=1, SLOPPY_CHROME=<path> (a Chrome binary instead
// of installed Chrome).
import { chromium } from "playwright";
import { execSync, spawnSync } from "node:child_process";
import { writeFileSync, mkdirSync } from "node:fs";
import os from "node:os";
import { headless, seedGame } from "./browser-helpers.mjs";

const [out, ...targets] = process.argv.slice(2);
const builds = targets.map((arg) => {
  const [label, url] = arg.split("=");
  return { label, url };
});
const env = process.env;
const scenes = (env.AB_SCENES ?? "village,harbor,stress-test").split(",");
const rounds = Number(env.AB_ROUNDS ?? 2);
const warmupFrames = Number(env.AB_WARMUP ?? 120);
const timedFrames = Number(env.AB_FRAMES ?? 300);
const width = Number(env.AB_WIDTH ?? 1280);
const height = Number(env.AB_HEIGHT ?? 720);
const seed = Number(env.AB_SEED ?? 12345);
const countCalls = env.AB_COUNT_CALLS === "1";
const profileDir = env.AB_PROFILE;
const camera = env.AB_CAMERA ?? "overview";
const pin = env.AB_PIN === "1" && os.platform() === "linux";
const STEP_MS = 1000 / 60;

const browser = await chromium.launch({
  ...(env.SLOPPY_CHROME ? { executablePath: env.SLOPPY_CHROME } : { channel: "chrome" }),
  headless,
  args: [
    `--window-size=${width},${height + 100}`,
    "--disable-backgrounding-occluded-windows",
    "--disable-renderer-backgrounding",
  ],
});
const results = {
  date: new Date().toISOString(),
  browser: browser.version(),
  environment: {
    cpus: os.cpus().length,
    cpu: os.cpus()[0]?.model,
    memoryGB: Math.round(os.totalmem() / 2 ** 30),
    platform: `${os.platform()} ${os.release()}`,
    gl: null,
  },
  settings: {
    scenes,
    rounds,
    warmupFrames,
    timedFrames,
    width,
    height,
    seed,
    countCalls,
    camera,
    pin,
  },
  builds,
  runs: [],
};
const save = () => writeFileSync(out, JSON.stringify(results, null, 1));

/** Pin every Chrome child process thread off core 0, then each renderer's main thread to it. */
function pinThreads() {
  if (!pin) return;
  const pids = execSync("pgrep -f -- '--type=' || true").toString().split("\n").filter(Boolean);
  for (const pid of pids) {
    // A process may have exited since pgrep listed it.
    spawnSync("taskset", ["-a", "-p", "-c", `1-${os.cpus().length - 1}`, pid], { stdio: "ignore" });
  }
  const renderers = execSync("pgrep -f -- '--type=renderer' || true")
    .toString()
    .split("\n")
    .filter(Boolean);
  for (const pid of renderers) {
    spawnSync("taskset", ["-p", "-c", "0", pid], { stdio: "ignore" });
  }
}

async function measure(build, scene) {
  const context = await browser.newContext({ viewport: { width, height }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.setDefaultTimeout(900_000);
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => {
    const text = m.text();
    if (
      m.type() === "error" ||
      (m.type() === "warning" && /WebGL: |GL_INVALID|GL ERROR/.test(text))
    )
      errors.push(text);
  });
  await seedGame(page, seed);
  await page.addInitScript((count) => {
    // Hold the game's loop (see scripts/browser-helpers.mjs freezeLoop).
    let loop;
    const requestFrame = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (callback) => {
      if (callback.name !== "loop") return requestFrame(callback);
      loop = callback;
      return 1;
    };
    window.__runLoop = (timestamp) => loop(timestamp);
    // Keep the engine's Wasm memory for reporting.
    const instantiate = WebAssembly.instantiate;
    WebAssembly.instantiate = async (...args) => {
      const result = await instantiate(...args);
      const instance = result.instance ?? result;
      if (instance?.exports?.memory) (window.__memories ??= []).push(instance.exports.memory);
      return result;
    };
    window.__glCalls = new Map();
    window.__glCounting = false;
    if (count) {
      const proto = WebGL2RenderingContext.prototype;
      for (const name of Object.getOwnPropertyNames(proto)) {
        const desc = Object.getOwnPropertyDescriptor(proto, name);
        if (typeof desc.value !== "function" || name === "constructor") continue;
        const original = desc.value;
        proto[name] = function (...args) {
          if (window.__glCounting)
            window.__glCalls.set(name, (window.__glCalls.get(name) ?? 0) + 1);
          return original.apply(this, args);
        };
      }
    }
  }, countCalls);
  const url = new URL(build.url);
  const extra = scene === "stress-test" || scene === "superstress" ? "&debug" : "";
  url.search = `?webgl&autoplay&map=${scene}${extra}`;
  const loadStart = Date.now();
  await page.goto(url.href);
  await page.waitForFunction(() => window.sloppy?.sim.match.phase === "playing", null, {
    polling: 500,
  });
  const loadSeconds = (Date.now() - loadStart) / 1000;
  pinThreads();
  await page.evaluate((overview) => {
    window.sloppy.overview(overview);
    const canvas = document.querySelector("canvas");
    window.__gl = canvas.getContext("webgl2");
    window.__time = performance.now();
  }, camera === "overview");
  const glInfo = await page.evaluate(() => {
    const gl = window.__gl;
    const info = gl.getExtension("WEBGL_debug_renderer_info");
    return info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);
  });
  results.environment.gl ??= glInfo;
  /** Run `frames` stepped frames; returns per-frame [renderMs, drawCalls] samples. */
  const step = (frames, timed) =>
    page.evaluate(
      async ({ frames, timed, stepMs }) => {
        const samples = [];
        for (let i = 0; i < frames; i++) {
          await new Promise((resolve) => requestAnimationFrame(resolve));
          window.__gl.finish();
          window.__time += stepMs;
          if (timed) window.__glCounting = true;
          window.__runLoop(window.__time);
          window.__glCounting = false;
          const stats = JSON.parse(window.sloppy.game.stats_json());
          samples.push([
            stats.renderMs,
            stats.drawCalls,
            stats.shadowDrawCalls,
            stats.reflectionDrawCalls,
          ]);
        }
        return samples;
      },
      { frames, timed, stepMs: STEP_MS },
    );
  await step(warmupFrames, false);
  const cdp = profileDir ? await context.newCDPSession(page) : null;
  if (cdp) {
    await cdp.send("Profiler.enable");
    await cdp.send("Profiler.setSamplingInterval", { interval: 100 });
    await cdp.send("Profiler.start");
  }
  await page.evaluate(() => window.__glCalls.clear());
  const samples = await step(timedFrames, true);
  if (cdp) {
    const { profile } = await cdp.send("Profiler.stop");
    mkdirSync(profileDir, { recursive: true });
    writeFileSync(`${profileDir}/${build.label}-${scene}.cpuprofile`, JSON.stringify(profile));
  }
  const calls = await page.evaluate((frames) => {
    const byName = {};
    let total = 0;
    for (const [name, n] of window.__glCalls) {
      byName[name] = n / frames;
      total += n;
    }
    return { perFrame: total / frames, byName };
  }, timedFrames);
  const after = await page.evaluate(() => ({
    stats: JSON.parse(window.sloppy.game.stats_json()),
    memory: (window.__memories ?? []).map((m) => m.buffer.byteLength),
    error: window.sloppy.game.error() ?? null,
    elapsed: window.sloppy.sim.elapsed,
  }));
  await context.close();
  const render = samples.map((s) => s[0]).sort((a, b) => a - b);
  const pct = (a, q) => a[Math.min(a.length - 1, Math.floor(a.length * q))] ?? 0;
  const mean = (a) => a.reduce((s, n) => s + n, 0) / Math.max(1, a.length);
  return {
    build: build.label,
    scene,
    loadSeconds,
    errors,
    calls: countCalls ? calls : null,
    renderMean: mean(render),
    renderP50: pct(render, 0.5),
    renderP90: pct(render, 0.9),
    renderP99: pct(render, 0.99),
    renderMin: render[0] ?? 0,
    renderMax: render.at(-1) ?? 0,
    drawCallsMean: mean(samples.map((s) => s[1])),
    shadowDrawCallsMean: mean(samples.map((s) => s[2])),
    reflectionDrawCallsMean: mean(samples.map((s) => s[3])),
    graphicsApi: after.stats.graphicsApi,
    latePipelines: after.stats.latePipelines,
    wasmMemory: after.memory,
    engineError: after.error,
    simElapsed: after.elapsed,
    samples,
  };
}

// ABBA...: alternate the build order every round.
for (let round = 0; round < rounds; round++) {
  const order = round % 2 === 0 ? builds : [...builds].reverse();
  for (const scene of scenes) {
    for (const build of order) {
      const run = await measure(build, scene);
      run.round = round;
      results.runs.push(run);
      save();
      const { samples, calls, ...summary } = run;
      console.log(
        JSON.stringify({
          ...summary,
          glCallsPerFrame: calls?.perFrame,
          samples: samples.length,
        }),
      );
    }
  }
}
await browser.close();
save();
