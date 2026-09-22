import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { chromium } from "playwright";

// Manual regression evidence: include startup and first-use effects, no warm-up
// discard and no sleeps in the simulation. Never put this workload in normal CI.
const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";
const seconds = Number(process.env.SLOPPY_PACING_SECONDS ?? 30);
const limit = Number(process.env.SLOPPY_MAX_FRAME_MS ?? 250);
const output = process.env.SLOPPY_ARTIFACT_DIR ?? "artifacts/performance/frame-pacing";
mkdirSync(output, { recursive: true });
const browser = await chromium.launch({ channel: "chrome", headless: false });
const results = [];
try {
  for (const map of ["village", "harbor", "quarry"]) {
    const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("console", (message) => {
      if (message.type() === "error") errors.push(message.text());
    });
    await page.addInitScript(() => {
      let seed = 12345;
      Math.random = () => (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296;
      window.framePacing = [];
      let previous = 0;
      const raf = requestAnimationFrame.bind(window);
      window.requestAnimationFrame = (callback) =>
        raf((time) => {
          if (callback.name !== "loop") {
            callback(time);
            return;
          }
          const start = performance.now();
          callback(time);
          const sim = window.sloppy?.sim;
          if (sim?.match.phase === "playing" && !document.hidden) {
            window.framePacing.push({
              gap: previous ? time - previous : 0,
              work: performance.now() - start,
              elapsed: sim.elapsed,
            });
            previous = time;
          } else {
            previous = 0;
          }
        });
    });
    await page.goto(`${url}?map=${map}`, { waitUntil: "domcontentloaded" });
    // Startup-check separately holds WASM/GPU work and exercises early GO.
    // Enable the same bot input before the first tick for reproducible combat.
    await page.waitForFunction(
      () => document.querySelector("#startup-overlay")?.dataset.state === "ready",
    );
    await page.evaluate(() => window.sloppy.autoplay());
    await page.locator("#start").scrollIntoViewIfNeeded();
    const box = await page.locator("#start").boundingBox();
    assert.ok(box);
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
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
    const result = await page.evaluate(() => {
      const samples = window.framePacing;
      const gaps = samples.map((sample) => sample.gap).sort((a, b) => a - b);
      return {
        samples,
        frames: samples.length,
        p95: gaps[Math.floor(gaps.length * 0.95)],
        p99: gaps[Math.floor(gaps.length * 0.99)],
        maxGap: Math.max(...gaps),
        maxWork: Math.max(...samples.map((sample) => sample.work)),
        shots: window.sloppy.sim.shotsFired,
      };
    });
    results.push({ map, seconds, ...result, errors });
    writeFileSync(`${output}/results.json`, JSON.stringify(results, null, 2));
    console.log(JSON.stringify({ map, ...result, samples: undefined, errors }));
    await page.close();
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
