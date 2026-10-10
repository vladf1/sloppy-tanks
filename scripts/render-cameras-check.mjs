// Every map drawn through moving, switched and fixed cameras: the arena's warm-up must
// have compiled every pipeline those views need (no late compile, no new pipeline), and
// switching cameras must leave no stale per-view state behind: the same still frame
// drawn before and after a detour through other views is the same image. Autoplay
// starts each round without Battle Setup; the check runs the real game loop at fixed
// frame steps and draws still frames through `Game.debug_render`.
import { freezeLoop, gameUrl as url, launchGame, pixels } from "./browser-helpers.mjs";
import fs from "node:fs";
import assert from "node:assert/strict";

const root = process.env.SLOPPY_ARTIFACT_DIR ?? "artifacts/performance/render-cameras";
const WIDTH = 1440;
const HEIGHT = 900;
/** Frames of autoplay with the following camera before the camera switches. */
const MOVING_FRAMES = 480;
const results = [];
fs.mkdirSync(root, { recursive: true });
const { browser, context, errors } = await launchGame({
  viewport: { width: WIDTH, height: HEIGHT },
  consoleErrors: true,
});

/** Share of pixels whose largest channel difference exceeds 20, and the largest. */
function difference(a, b) {
  let large = 0;
  let max = 0;
  for (let i = 0; i < a.length; i += 4) {
    let peak = 0;
    for (let c = 0; c < 3; c++) peak = Math.max(peak, Math.abs(a[i + c] - b[i + c]));
    if (peak > 20) large++;
    max = Math.max(max, peak);
  }
  return { largePercent: (100 * large) / (WIDTH * HEIGHT), max };
}

try {
  for (const map of ["village", "harbor", "quarry"]) {
    const page = await context.newPage();
    await freezeLoop(page);
    await page.goto(`${url}?autoplay&map=${map}`);
    await page.waitForFunction(() => window.sloppy?.sim.match.phase === "playing");
    await page.evaluate(() => {
      for (const element of document.querySelectorAll("#overlay, #hud")) {
        element.style.display = "none";
      }
    });
    const prepared = await page.evaluate(() => window.sloppy.stats());
    const counters = () =>
      page.evaluate(() => {
        const { drawCalls, pipelines, latePipelines } = window.sloppy.stats();
        return { drawCalls, pipelines, latePipelines, error: window.sloppy.error() };
      });
    const phases = {};
    // A moving camera: autoplay drives the followed tank through the real loop.
    phases.moving = await page.evaluate(async (frames) => {
      for (let i = 0; i < frames; i++) {
        window.advanceFrame(1000 / 60);
        if (i % 60 === 0) await new Promise(requestAnimationFrame);
      }
      return window.sloppy.sim.elapsed;
    }, MOVING_FRAMES);
    phases.afterMoving = await counters();
    const still = async (name) => {
      await page.evaluate(() => window.engine.draw());
      return pixels(await page.screenshot({ path: `${root}/${map}-${name}.png` }));
    };
    const before = await still("follow-before");
    // A still detour (no time passes, so nothing animates): the overview and fixed
    // poses far from the follow camera, then the same following frame again.
    await page.evaluate(() => {
      const { engine } = window;
      engine.draw([], true);
      engine.draw([], true);
      for (const pose of [
        [0, 160, 1, 0, 0, 0],
        [-60, 6, 60, 0, 2, 0],
        [60, 25, -60, -20, 0, 20],
      ]) {
        engine.draw(pose);
      }
    });
    phases.afterDetour = await counters();
    const after = await still("follow-after");
    const detour = difference(before, after);
    // Views that need time: first person flies into the turret; extreme zooms.
    const drawFrames = (count) =>
      page.evaluate((count) => {
        for (let i = 0; i < count; i++) {
          window.sloppy.game.debug_render(1, 1 / 60, false, new Float32Array());
        }
      }, count);
    await page.evaluate(() => window.sloppy.firstPerson());
    await drawFrames(90);
    await page.screenshot({ path: `${root}/${map}-first-person.png` });
    phases.afterFirstPerson = await counters();
    await page.evaluate(() => window.sloppy.firstPerson());
    await drawFrames(90);
    const { minZoom, maxZoom } = await page.evaluate(() => window.sloppy.view);
    for (const zoom of [minZoom, maxZoom]) {
      await page.evaluate((zoom) => window.sloppy.zoom(zoom), zoom);
      await drawFrames(30);
      await page.screenshot({ path: `${root}/${map}-zoom-${zoom}.png` });
    }
    phases.afterZoom = await counters();
    const result = { map, prepared: prepared.pipelines, phases, detour, errors: [...errors] };
    results.push(result);
    console.log(JSON.stringify(result));
    fs.writeFileSync(`${root}/results.json`, JSON.stringify(results, null, 2));
    assert.ok(phases.moving > 5, `${map}: autoplay played`);
    for (const [name, phase] of Object.entries(phases)) {
      if (typeof phase !== "object") continue;
      assert.equal(phase.error, null, `${map} ${name}: GPU error`);
      assert.equal(phase.latePipelines, 0, `${map} ${name}: a pipeline compiled mid-round`);
      assert.equal(phase.pipelines, prepared.pipelines, `${map} ${name}: new pipelines`);
      assert.ok(phase.drawCalls > 0, `${map} ${name}: nothing drawn`);
    }
    assert.ok(
      detour.largePercent < 0.04,
      `${map}: the same frame differs after switching cameras (${detour.largePercent}%)`,
    );
    await page.close();
  }
  assert.deepEqual(errors, []);
} finally {
  await browser.close();
}
