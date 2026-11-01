// The page in Playwright's WebKit (Safari's engine) at an iPad's size (1180×820 at 2×,
// a touch screen) and a desktop's (1280×800): the shell starts on WebGPU where WebKit
// offers an adapter (else on WebGL2), Battle Setup's arena gets ready, a click on GO
// plays, and W drives the tank at the display's frame rate with no pipeline compiled
// mid-round and no renderer, page or console error. While driving it records the
// animation frames' gaps and the presented frames' (the shell's clock per frame) and
// prints their distribution; headless timing is noisy, so only a gross stall fails.
import assert from "node:assert/strict";
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
/** Frames per second the drive must reach: headless WebKit paces to 60, and WebKit's
 * WebGPU canvas drops to 30 or 24 when a frame's work overruns the display's. */
const MIN_DRIVE_FPS = 45;
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

const browser = await webkit.launch({ headless });
const results = { browser: `WebKit ${browser.version()}` };
try {
  for (const { name, options } of SIZES) {
    const context = await browser.newContext(options);
    // Every animation frame's time, and the shell's clock for each frame it presented.
    await context.addInitScript(() => {
      const pacing = (window.pacing = { recording: false, frames: [], presented: [] });
      let shown = 0;
      const request = window.requestAnimationFrame.bind(window);
      window.requestAnimationFrame = (callback) =>
        request((time) => {
          callback(time);
          if (!pacing.recording) return;
          pacing.frames.push(time);
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
      return { frames: window.pacing.frames, presented: window.pacing.presented };
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
    };
    results[name] = result;
    writeFileSync(`${out}/${name}-samples.json`, JSON.stringify(pacing));
    console.log(`webkit-check: ${name}: ${JSON.stringify(result)}`);
    await page.screenshot({ path: `${out}/${name}-driving.png` });

    assert.ok(moved > 1, `${name}: W drives the tank, ${moved.toFixed(2)} m`);
    assert.ok(fps >= MIN_DRIVE_FPS, `${name}: ${fps.toFixed(1)} frames per second while driving`);
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
