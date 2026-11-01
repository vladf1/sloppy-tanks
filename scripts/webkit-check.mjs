// The page in Playwright's WebKit (Safari's engine) at an iPad's size (1180×820 at 2×,
// a touch screen) and a desktop's (1280×800): the shell starts on WebGPU where WebKit
// offers an adapter (else on WebGL2), Battle Setup's arena gets ready, a click on GO
// plays, and W drives the tank at the display's frame rate with no pipeline compiled
// mid-round and no renderer, page or console error. While driving it records the
// animation frames' gaps and the presented frames' (the shell's clock per frame) and
// prints their distribution; headless timing is noisy, so only a gross stall fails.
//
// The frame rate is judged against the machine, not a fixed number: a blank page first
// shows the rate WebKit gives any page here (60 headless on a Mac, 30 in macOS's Low
// Power Mode, more on a fast display), and the drive must keep most of it. A drive held
// lower while its frames fit a display frame is WebKit's canvas pacing (bug 325095,
// which `reset_canvas_pacing` undoes) and fails; one whose frames cost more is this
// machine being slower than its display, printed but not judged: timing is not a gate.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { setTimeout as wait } from "node:timers/promises";
import { webkit } from "playwright";
import {
  clickWidget,
  collectErrors,
  headless,
  nextFrame,
  openShell,
  untilState,
} from "./browser-helpers.mjs";

const out = "artifacts/performance/webkit-check";
mkdirSync(out, { recursive: true });
/** Preparing a warm arena takes a moment; a cold Metal shader cache takes longer. */
const READY_TIMEOUT_MS = 180_000;
/** How long W is held while frames are counted. */
const DRIVE_MS = 5000;
/** The drive's first stretch, whose frames may still settle in. */
const SETTLE_MS = 1000;
/** The share of the blank page's frame rate the drive must keep: WebKit's WebGPU canvas
 * drops to a half or less (60 to 30 or 24) when one frame's work overruns. */
const MIN_DRIVE_SHARE = 0.75;
/** The share of a frame its work fills when the work, not the browser, sets the rate:
 * the main thread's of a display frame, the GPU's of the drive's own frame interval.
 * WebKit queues frames the GPU is behind on, so a GPU-bound page runs at its GPU time
 * rather than a vsync step, while an Apple GPU under a held canvas clocks down to fill
 * only about 0.6-0.75 of it (21-25 ms of 33, `quality.rs`). */
const BOUND_SHARE = 0.85;
/** A frame gap this long after the first second is a stall, not noise. */
const STALL_MS = 250;
/** Distinct pipelines an arena may prepare; the village needs about 60. */
const MAX_PIPELINES = 150;

const SIZES = [
  {
    name: "ipad",
    options: { viewport: { width: 1180, height: 820 }, deviceScaleFactor: 2, hasTouch: true },
  },
  { name: "desktop", options: { viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 } },
];

const percentile = (sorted, q) =>
  sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))] ?? 0;
/** p50/p95/p99/max and the counts over 20 and 33 ms of `values` (ms). */
function distribution(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const round = (value) => Math.round(value * 10) / 10;
  return {
    samples: sorted.length,
    p50: round(percentile(sorted, 0.5)),
    p95: round(percentile(sorted, 0.95)),
    p99: round(percentile(sorted, 0.99)),
    max: round(sorted.at(-1) ?? 0),
    over20: sorted.filter((value) => value > 20).length,
    over33: sorted.filter((value) => value > 33.4).length,
  };
}
const gaps = (times) => times.slice(1).map((time, index) => time - times[index]);

/** Animation frames a blank page times before the game is judged. */
const BLANK_PAGE_FRAMES = 60;
/** The frame rate WebKit gives a page that draws nothing: the most the game can reach. */
async function blankPageFps(browser) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const times = await page.evaluate(
    (count) =>
      new Promise((resolve) => {
        const times = [];
        const tick = (time) => {
          times.push(time);
          if (times.length > count) resolve(times);
          else requestAnimationFrame(tick);
        };
        requestAnimationFrame(tick);
      }),
    BLANK_PAGE_FRAMES,
  );
  await context.close();
  return (BLANK_PAGE_FRAMES * 1000) / (times.at(-1) - times[0]);
}

/** Why the browser may hold every page, where the system says: Low Power Mode is
 * `powermode 1` on Apple silicon and `lowpowermode 1` on Intel Macs. */
function lowPowerMode() {
  if (process.platform !== "darwin") return false;
  const settings = execFileSync("pmset", ["-g"], { encoding: "utf8" });
  return /^\s*(?:powermode|lowpowermode)\s+1\b/m.test(settings);
}

const browser = await webkit.launch({ headless });
const results = { browser: `WebKit ${browser.version()}` };
try {
  const displayFps = await blankPageFps(browser);
  results.blankPageFps = Math.round(displayFps * 10) / 10;
  const lowPower = lowPowerMode() ? " (Low Power Mode is on)" : "";
  console.log(`webkit-check: a blank page runs at ${results.blankPageFps} fps${lowPower}`);
  for (const { name, options } of SIZES) {
    const context = await browser.newContext(options);
    // Every animation frame's time, and the shell's clock for each frame it presented.
    await context.addInitScript(() => {
      const pacing = (window.pacing = { recording: false, frames: [], presented: [], work: [] });
      let shown = 0;
      const request = window.requestAnimationFrame.bind(window);
      window.requestAnimationFrame = (callback) =>
        request((time) => {
          const start = performance.now();
          callback(time);
          if (!pacing.recording) return;
          pacing.frames.push(time);
          pacing.work.push(performance.now() - start);
          const state = window.sloppyShell?.state();
          if (state && state.frame !== shown) {
            shown = state.frame;
            pacing.presented.push(state.now);
          }
        });
    });
    const page = await context.newPage();
    const errors = [];
    collectErrors(page, errors, { consoleErrors: true, prefix: `${name}: ` });
    // WebKit never hands a preload's response to a later request, so a texture the page
    // starts early must reach the engine as that same download.
    const textures = new Map();
    page.on("request", (request) => {
      const path = new URL(request.url()).pathname;
      if (path.includes("/textures/")) textures.set(path, (textures.get(path) ?? 0) + 1);
    });
    const started = Date.now();
    await openShell(page);
    await untilState(page, (state) => state.ready, {
      timeout: READY_TIMEOUT_MS,
      what: `${name}: the arena ready`,
    });
    const readyMs = Date.now() - started;
    const webgpu = await page.evaluate(async () =>
      Boolean(await navigator.gpu?.requestAdapter().catch(() => null)),
    );
    assert.ok(textures.size > 0, `${name}: the arena downloaded textures`);
    assert.deepEqual(
      [...textures].filter(([, count]) => count > 1),
      [],
      `${name}: each texture downloaded once`,
    );
    await page.screenshot({ path: `${out}/${name}-setup.png` });

    await clickWidget(page, "GO!");
    let state = await untilState(page, (state) => state.inRound && state.phase === "playing", {
      extras: ["debug", "nerd"],
      what: `${name}: the round`,
    });
    const rows = state.extras.nerd.flatMap((section) => section.rows ?? []);
    const graphics = rows.find((row) => row.label === "Graphics API")?.value;
    const expected = webgpu && !process.env.SLOPPY_WEBGL ? "WebGPU" : "WebGL";
    assert.equal(graphics, expected, `${name}: Graphics API`);
    const before = state.extras.debug.human;

    await page.mouse.move(options.viewport.width * 0.7, options.viewport.height * 0.3);
    await page.evaluate(() => (window.pacing.recording = true));
    const driveStart = await page.evaluate(() => ({
      frame: window.sloppyShell.state().frame,
      time: performance.now(),
    }));
    await page.keyboard.down("KeyW");
    await wait(DRIVE_MS);
    await page.keyboard.up("KeyW");
    const driveEnd = await page.evaluate(() => ({
      frame: window.sloppyShell.state().frame,
      time: performance.now(),
    }));
    const pacing = await page.evaluate(() => {
      window.pacing.recording = false;
      const { frames, presented, work } = window.pacing;
      return { frames, presented, work };
    });
    state = await nextFrame(page, ["debug", "stats"]);
    const moved = Math.hypot(
      state.extras.debug.human.x - before.x,
      state.extras.debug.human.z - before.z,
    );
    const fps = ((driveEnd.frame - driveStart.frame) * 1000) / (driveEnd.time - driveStart.time);
    // Gaps after the drive's first second; the ones before it are kept in the record.
    const settled = (times) => times.filter((time) => time - times[0] >= SETTLE_MS);
    const result = {
      backend: graphics,
      readyMs,
      textures: textures.size,
      fps: Math.round(fps * 10) / 10,
      moved: Math.round(moved * 100) / 100,
      pipelines: state.extras.stats.pipelines,
      latePipelines: state.extras.stats.latePipelines,
      animationFrameGaps: distribution(gaps(pacing.frames)),
      presentedGaps: distribution(gaps(pacing.presented)),
      settledMaxGap: Math.max(...gaps(settled(pacing.frames)), ...gaps(settled(pacing.presented))),
      // The main thread's animation-frame callbacks (engine, egui and drawing) and the
      // GPU's smoothed frame time, null where WebKit gives no timestamps.
      frameWork: distribution(pacing.work),
      gpuMs: state.extras.stats.quality.gpuMs,
      pacingResets: state.extras.stats.quality.pacingResets,
    };
    results[name] = result;
    writeFileSync(`${out}/${name}-samples.json`, JSON.stringify(pacing));
    console.log(`webkit-check: ${name}: ${JSON.stringify(result)}`);
    await page.screenshot({ path: `${out}/${name}-driving.png` });

    assert.ok(moved > 1, `${name}: W drives the tank, ${moved.toFixed(2)} m`);
    if (fps < displayFps * MIN_DRIVE_SHARE) {
      const displayMs = 1000 / displayFps;
      const cpuMs = result.frameWork.p50;
      const gpuMs = result.gpuMs ?? 0;
      const bound = cpuMs > displayMs * BOUND_SHARE || gpuMs > (1000 / fps) * BOUND_SHARE;
      const cost = `main thread ${cpuMs.toFixed(1)} ms, GPU ${gpuMs.toFixed(1)} ms`;
      assert.ok(
        bound,
        `${name}: ${fps.toFixed(1)} fps while driving against a blank page's ` +
          `${displayFps.toFixed(1)} with frames to spare (${cost}): WebKit's canvas pacing held them`,
      );
      console.log(
        `webkit-check: ${name}: ${fps.toFixed(1)} fps, bound by this machine (${cost}); ` +
          `the frame rate is not judged`,
      );
    }
    assert.ok(result.pipelines <= MAX_PIPELINES, `${name}: ${result.pipelines} pipelines`);
    assert.equal(result.latePipelines, 0, `${name}: pipelines compiled while playing`);
    assert.ok(
      result.settledMaxGap < STALL_MS,
      `${name}: a ${Math.round(result.settledMaxGap)} ms frame gap while driving`,
    );
    assert.equal(state.error, null, `${name}: renderer errors`);
    assert.deepEqual(errors, [], `${name}: page and console errors`);
    await context.close();
  }
  console.log("webkit-check: startup, GO, driving frame rate and pacing passed");
} finally {
  writeFileSync(`${out}/results.json`, JSON.stringify(results, null, 2));
  await browser.close();
}
