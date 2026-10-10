// Headless check of the render lab: startup, warm-up, the comparison with the captured
// Three.js references (when present), joint overrides, frustum culling, resizing,
// fading without late pipelines, repeated scene reloads (resources must not grow),
// picking, and GPU/console errors. Needs the labs engine (`pnpm run wasm:labs`)
// and a dev server:
//   pnpm exec vite --host 127.0.0.1 --port 5190 --strictPort
//   RENDER_LAB_URL=http://127.0.0.1:5190/sloppy-tanks/tools/render-lab.html node tools/render-lab-check.mjs
// Screenshots and the report go to artifacts/performance/render-lab/. With the
// references, the mean error of each pose must stay within `RENDER_LAB_TOLERANCE`
// (default 1.0 of 255) of the Three.js frames.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { launchLab, meanError, openLab, shoot, within } from "./lab-check.mjs";

const url =
  process.env.RENDER_LAB_URL ?? "http://127.0.0.1:5190/sloppy-tanks/tools/render-lab.html";
/** The references were captured at this effect/water time. */
const REFERENCE_TIME = "1.25";
const tolerance = Number(process.env.RENDER_LAB_TOLERANCE ?? 1);
const output = fileURLToPath(new URL("../artifacts/performance/render-lab/", import.meta.url));
await mkdir(output, { recursive: true });
const { browser, page, errors } = await launchLab();
const report = {};
try {
  // The lab engine's Wasm memory, to grow it by hand.
  await page.addInitScript(() => {
    for (const name of ["instantiate", "instantiateStreaming"]) {
      const original = WebAssembly[name];
      WebAssembly[name] = async (...args) => {
        const result = await original(...args);
        window.labMemory = (result.instance ?? result).exports.memory;
        return result;
      };
    }
  });
  await openLab(page, `${url}?freeze=${REFERENCE_TIME}`);
  report.comparison = await page.evaluate(() => window.renderLab.compare());
  report.stats = await page.evaluate(() => window.renderLab.stats());
  within(report.comparison, "default", tolerance);
  await shoot(page, output, "default");
  await page.screenshot({ path: `${output}page.png` });
  // The remaining references were captured with the second tank's turret turned.
  await page.evaluate(() => window.renderLab.poseJoint("tank", 1, "turret", 1.2));
  // The game's default overhead pose (zoom 34): shadows and PBR.
  report.overhead = await page.evaluate(() => window.renderLab.usePose("overhead"));
  within(report.overhead, "overhead", tolerance);
  await shoot(page, output, "overhead");
  // A close-up of the reflection, fog and effects from lower down.
  report.closeUp = await page.evaluate(() => window.renderLab.usePose("close"));
  within(report.closeUp, "close-up", tolerance);
  await shoot(page, output, "close");
  // Looking away from the scene culls nearly everything.
  await page.evaluate(() => window.renderLab.setCamera([0, 9, 30], [0, 12, 80]));
  await page.evaluate(() => window.renderLab.compare());
  report.culled = await page.evaluate(() => window.renderLab.stats());
  assert.ok(report.culled.drawCalls < report.stats.drawCalls / 2, "frustum culling drops draws");
  // Resizing replaces the attachments; the image stays matched.
  report.resized = await page.evaluate(() => window.renderLab.usePose("resized"));
  within(report.resized, "resized", tolerance);
  await shoot(page, output, "resized");
  await page.evaluate(() => window.renderLab.usePose("default"));
  // Fading uses the pre-compiled blended variant; no pipeline compiles late.
  await page.evaluate(() => window.renderLab.setOpacity("hull", 0.5));
  await page.evaluate(() => window.renderLab.compare());
  await page.locator("#rust").screenshot({ path: `${output}faded-hull.png` });
  report.afterFade = await page.evaluate(() => window.renderLab.stats());
  assert.equal(report.afterFade.latePipelines, 0, "fade variant was warmed up");
  // Reloading the scene as new rounds must not grow resources.
  const first = await page.evaluate(() => window.renderLab.reload());
  let last = first;
  for (let i = 0; i < 8; i++) {
    last = await page.evaluate(() => window.renderLab.reload());
  }
  report.reload = { first, last };
  for (const key of [
    "meshes",
    "materials",
    "models",
    "instances",
    "drawClasses",
    "textures",
    "gpuBytes",
  ]) {
    assert.equal(last[key], first[key], `${key} stays bounded across reloads`);
  }
  // Picking the look target through the canvas centre.
  report.pick = await page.evaluate(() => window.renderLab.pick(320, 200, 1.5));
  // Generated pixels reach the GPU a frame after they are supplied: they must
  // survive the Wasm memory growing in between, and replacing them must redraw the
  // materials that already show them (the leaf cards, here in opaque red, then green).
  const replaced = [];
  for (const rgba of [
    [255, 0, 0, 255],
    [0, 255, 0, 255],
  ]) {
    await page.evaluate((rgba) => {
      window.renderLab.setGeneratedTexture("lab-leaf", 1, 1, new Uint8Array(rgba));
      window.labMemory.grow(1);
      window.renderLab.compare();
    }, rgba);
    replaced.push(await page.locator("#rust").screenshot());
  }
  assert.notDeepEqual(replaced[0], replaced[1], "replaced generated pixels redraw");
  report.textureFailures = await page.evaluate(() => window.renderLab.textureFailures());
  report.gpuError = await page.evaluate(() => window.renderLab.error());
  // Its expected startup error is reported to the console; keep it out of `errors`.
  const reported = errors.length;
  const unavailable = await browser.newPage();
  await unavailable.addInitScript(() =>
    Object.defineProperty(navigator, "gpu", { value: undefined }),
  );
  await unavailable.goto(url);
  await unavailable.waitForFunction(() => document.body.dataset.state === "error");
  report.unavailable = await unavailable.locator("#status").textContent();
  assert.match(report.unavailable, /WebGPU is required/);
  await unavailable.close();
  assert.ok(errors.slice(reported).every((error) => /WebGPU is required/.test(error)));
  errors.length = reported;
  report.consoleErrors = errors;
  await writeFile(`${output}report.json`, JSON.stringify(report, null, 2));
  assert.deepEqual(report.textureFailures, []);
  assert.equal(report.gpuError, null);
  assert.deepEqual(errors, []);
  console.log(
    `PASS: mean |Δ| vs Three.js ${meanError(report.comparison)} (close-up ${meanError(report.closeUp)}), ` +
      `overhead ${meanError(report.overhead)}, resized ${meanError(report.resized)}; ` +
      `${report.stats.drawCalls} draws (${report.culled.drawCalls} culled view), ${report.stats.pipelines} pipelines. ` +
      `Report: ${output}report.json`,
  );
} catch (error) {
  report.failure = String(error?.stack ?? error);
  report.consoleErrors = errors;
  await writeFile(`${output}report.json`, JSON.stringify(report, null, 2));
  await page.screenshot({ path: `${output}failure.png` }).catch(() => {});
  throw error;
} finally {
  await browser.close();
}
