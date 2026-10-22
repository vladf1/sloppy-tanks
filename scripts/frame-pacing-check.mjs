import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { chromium, webkit } from "playwright";
import { click, collectErrors, gameUrl, headless, menuReady } from "./browser-helpers.mjs";

// Manual regression evidence: include startup and first-use effects, no warm-up
// discard and no sleeps in the simulation. Never put this workload in normal CI.
const seconds = Number(process.env.SLOPPY_PACING_SECONDS ?? 30);
const limit = Number(process.env.SLOPPY_MAX_FRAME_MS ?? 250);
const output = process.env.SLOPPY_ARTIFACT_DIR ?? "artifacts/performance/frame-pacing";
const maps = (process.env.SLOPPY_PACING_MAPS ?? "village,harbor,quarry").split(",");
/** `webkit` measures Playwright's WebKit (Safari's engine) instead of Chrome. */
const inWebKit = process.env.SLOPPY_BROWSER === "webkit";
/** An iPad-sized touch screen (1180x820 CSS at 2x) instead of a 1280x720 desktop. */
const tablet = Boolean(process.env.SLOPPY_TABLET);
/** Chrome's CPU slowdown, a stand-in for a tablet's CPU. */
const throttle = Number(process.env.SLOPPY_CPU_THROTTLE ?? 1);
mkdirSync(output, { recursive: true });
const browser = inWebKit
  ? await webkit.launch({ headless })
  : await chromium.launch({ channel: "chrome", headless });

const percentile = (sorted, q) =>
  sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))];
/** p50/p95/p99/max and the count over 20 and 33 ms of `values` (ms). */
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

const results = [];
try {
  for (const map of maps) {
    const context = await browser.newContext(
      tablet
        ? { viewport: { width: 1180, height: 820 }, deviceScaleFactor: 2, hasTouch: true }
        : { viewport: { width: 1280, height: 720 } },
    );
    const page = await context.newPage();
    if (!inWebKit && throttle > 1) {
      const cdp = await context.newCDPSession(page);
      await cdp.send("Emulation.setCPUThrottlingRate", { rate: throttle });
    }
    const errors = [];
    collectErrors(page, errors, { consoleErrors: true });
    await page.addInitScript(() => {
      let seed = 12345;
      Math.random = () => (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296;
      window.framePacing = [];
      // Whether a loop frame drew (submitted the canvas's command buffer) and when the
      // GPU finished it: WebKit can run frames far ahead of the GPU.
      let current;
      const submit = globalThis.GPUQueue?.prototype.submit;
      if (submit) {
        GPUQueue.prototype.submit = function (buffers) {
          submit.call(this, buffers);
          const sample = current;
          if (sample && buffers.some((buffer) => buffer.label === "output")) {
            sample.drew = true;
            const start = performance.now();
            this.onSubmittedWorkDone().then(() => (sample.gpuDoneMs = performance.now() - start));
          }
        };
      }
      let previous = 0;
      const raf = requestAnimationFrame.bind(window);
      window.requestAnimationFrame = (callback) =>
        raf((time) => {
          if (callback.name !== "loop") {
            callback(time);
            return;
          }
          const start = performance.now();
          const sample = { gap: previous ? time - previous : 0, time, drew: false };
          current = sample;
          callback(time);
          current = undefined;
          sample.work = performance.now() - start;
          const sim = window.sloppy?.sim;
          if (sim?.match.phase === "playing" && !document.hidden) {
            sample.elapsed = sim.elapsed;
            window.framePacing.push(sample);
            previous = time;
          } else {
            previous = 0;
          }
        });
    });
    await page.goto(`${gameUrl}?map=${map}`, { waitUntil: "domcontentloaded" });
    // Startup-check separately holds WASM/GPU work and exercises early GO.
    // Enable the same bot input before the first tick for reproducible combat.
    await menuReady(page, { timeout: 180_000 });
    await page.evaluate(() => window.sloppy.autoplay());
    await click(page, "#start");
    try {
      await page.waitForFunction(() => window.sloppy?.sim.match.phase === "playing");
    } catch (error) {
      console.error(
        await page.evaluate(() => ({
          state: document.querySelector("#startup-overlay")?.dataset.state,
          status: document.querySelector("#startup-status")?.textContent,
          phase: window.sloppy?.sim.match.phase,
        })),
        errors,
      );
      await page.screenshot({ path: `${output}/startup-failure-${map}.png` });
      throw error;
    }
    await page.waitForFunction((duration) => window.sloppy.sim.elapsed >= duration, seconds, {
      timeout: (seconds + 40) * 1000,
    });
    // Let the last frames' GPU work report back.
    await page.waitForTimeout(1000);
    const { samples, shots } = await page.evaluate(() => ({
      samples: window.framePacing,
      shots: window.sloppy.sim.shotsFired,
    }));
    const drawn = samples.filter((sample) => sample.drew);
    const result = {
      map,
      seconds,
      browser: inWebKit ? "webkit" : "chrome",
      tablet,
      throttle,
      frames: samples.length,
      gaps: distribution(samples.slice(1).map((sample) => sample.gap)),
      drawnGaps: distribution(drawn.slice(1).map((sample, i) => sample.time - drawn[i].time)),
      work: distribution(samples.map((sample) => sample.work)),
      gpuLatency: distribution(drawn.map((sample) => sample.gpuDoneMs).filter(Number.isFinite)),
      maxGap: Math.max(...samples.map((sample) => sample.gap)),
      maxWork: Math.max(...samples.map((sample) => sample.work)),
      shots,
    };
    results.push({ ...result, errors });
    writeFileSync(
      `${output}/results.json`,
      JSON.stringify(
        results.map((entry) => ({ ...entry, samples: undefined })),
        null,
        2,
      ),
    );
    writeFileSync(`${output}/samples-${map}.json`, JSON.stringify(samples));
    console.log(JSON.stringify({ ...result, errors }));
    await context.close();
  }
  for (const result of results) {
    assert.deepEqual(result.errors, [], `${result.map}: browser errors`);
    assert.ok(result.shots > 10, `${result.map}: combat must run`);
    assert.ok(
      result.maxGap < limit && result.maxWork < limit,
      `${result.map}: frame exceeds ${limit} ms`,
    );
  }
} finally {
  await browser.close();
}
