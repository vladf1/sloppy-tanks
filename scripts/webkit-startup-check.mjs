// The game in WebKit (Safari's engine) with WebGPU: Battle Setup becomes ready on the
// village and the quarry with a sane count of distinct pipelines, a physical click on
// GO starts the round, and keyboard driving and a mouse shot reach the simulation,
// without page, console or GPU errors.
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { webkit } from "playwright";
import { gameUrl, headless, installEngineHelpers } from "./browser-helpers.mjs";

const output = "artifacts/performance/webkit-startup";
mkdirSync(output, { recursive: true });
/** Distinct pipelines an arena may prepare; the maps need about 45-55. */
const MAX_PIPELINES = 150;
/** Preparing a warm arena takes about a second; a cold shader cache takes longer. */
const READY_TIMEOUT_MS = 120_000;

const browser = await webkit.launch({ headless });
const errors = [];
const results = {};
try {
  for (const map of ["village", "quarry"]) {
    const context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      deviceScaleFactor: 1,
    });
    await installEngineHelpers(context);
    const page = await context.newPage();
    page.on("pageerror", (error) => errors.push(`${map}: ${error.message}`));
    page.on("console", (message) => {
      if (message.type() === "error") errors.push(`${map}: ${message.text()}`);
    });
    // Every startup stage Battle Setup reports, to read the shader progress total.
    await page.addInitScript(() => {
      window.startupStages = [];
      new MutationObserver(() => {
        const stage = document.querySelector("#startup-status")?.textContent;
        if (stage && window.startupStages.at(-1) !== stage) window.startupStages.push(stage);
      }).observe(document, { subtree: true, childList: true, characterData: true });
    });
    const started = Date.now();
    await page.goto(`${gameUrl}?map=${map}`, { waitUntil: "domcontentloaded" });
    await page.waitForFunction(
      () => document.querySelector("#startup-overlay")?.dataset.state === "ready",
      null,
      { timeout: READY_TIMEOUT_MS },
    );
    const readyMs = Date.now() - started;
    const prepared = await page.evaluate(() => ({
      userAgent: navigator.userAgent,
      map: window.sloppy.sim.mapMode,
      stages: window.startupStages,
      stats: window.engine.stats(),
      gpuError: window.sloppy.error(),
    }));
    assert.match(prepared.userAgent, /AppleWebKit/);
    assert.equal(prepared.map, map);
    assert.equal(prepared.gpuError, null);
    const totals = prepared.stages
      .map((stage) => /Shaders loaded: \d+ of (\d+)/.exec(stage)?.[1])
      .filter(Boolean)
      .map(Number);
    assert.ok(totals.length > 0, `shader progress shown: ${prepared.stages.join(" | ")}`);
    const shaderTotal = Math.max(...totals);
    assert.ok(shaderTotal <= MAX_PIPELINES, `shader progress total ${shaderTotal}`);
    assert.ok(
      prepared.stats.pipelines > 0 && prepared.stats.pipelines <= MAX_PIPELINES,
      `prepared pipelines ${prepared.stats.pipelines}`,
    );
    assert.equal(prepared.stats.latePipelines, 0);

    // A physical click on GO, as a player starts a round.
    const go = await page.locator("#start").boundingBox();
    assert.ok(go);
    await page.mouse.click(go.x + go.width / 2, go.y + go.height / 2);
    await page.waitForFunction(
      () =>
        !document.querySelector("#startup-overlay") &&
        window.sloppy.sim.match.phase === "playing" &&
        window.sloppy.view.time > 0,
    );
    const before = await page.evaluate(() => window.sloppy.sim.human);
    const framesBefore = await page.evaluate(() => window.sloppy.frames);
    await page.mouse.move(900, 300);
    await page.keyboard.down("w");
    await page.waitForTimeout(1500);
    await page.keyboard.up("w");
    await page.mouse.down();
    await page.waitForFunction(() => window.sloppy.sim.shotsFired > 0, null, { timeout: 5000 });
    await page.mouse.up();
    const after = await page.evaluate(() => ({
      human: window.sloppy.sim.human,
      shots: window.sloppy.sim.shotsFired,
      frames: window.sloppy.frames,
      gpuError: window.sloppy.error(),
      stats: window.engine.stats(),
    }));
    const moved = Math.hypot(after.human.x - before.x, after.human.z - before.z);
    assert.ok(moved > 1, `W drives the tank: ${moved.toFixed(2)} m`);
    // Headless WebKit paces animation frames unevenly (15-30 per second here).
    assert.ok(after.frames - framesBefore > 10, `frames ran: ${after.frames - framesBefore}`);
    assert.equal(after.gpuError, null);
    assert.equal(after.stats.latePipelines, 0, "no pipeline compiled while playing");
    await page.screenshot({ path: `${output}/${map}.png` });
    results[map] = {
      readyMs,
      shaderTotal,
      pipelines: prepared.stats.pipelines,
      moved: Number(moved.toFixed(2)),
      shots: after.shots,
      frames: after.frames - framesBefore,
    };
    results.userAgent = prepared.userAgent;
    await context.close();
  }
  assert.deepEqual(errors, []);
  writeFileSync(`${output}/checks.json`, JSON.stringify({ ...results, errors }, null, 2));
  console.log(JSON.stringify(results));
  console.log("WebKit startup check passed.");
} finally {
  await browser.close();
}
