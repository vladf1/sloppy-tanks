// Headless check of the render lab: startup, warm-up, the Rust/Three.js pixel
// comparison, repeated scene reloads (resources must not grow), picking, and
// GPU/console errors. Needs the engine built (`pnpm run wasm`) and a dev server:
//   pnpm exec vite --host 127.0.0.1 --port 5190 --strictPort
//   RENDER_LAB_URL=http://127.0.0.1:5190/sloppy-tanks/tools/render-lab.html node tools/render-lab-check.mjs
// Screenshots and the report go to artifacts/performance/render-lab/.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { launchGame } from "../scripts/browser-helpers.mjs";

const url =
  process.env.RENDER_LAB_URL ?? "http://127.0.0.1:5190/sloppy-tanks/tools/render-lab.html";
const freeze = process.env.RENDER_LAB_TIME ?? "1.25";
const output = fileURLToPath(new URL("../artifacts/performance/render-lab/", import.meta.url));
await mkdir(output, { recursive: true });
const { browser, page, errors } = await launchGame({
  viewport: { width: 2000, height: 1100 },
  consoleErrors: true,
});
const report = {};
try {
  await page.goto(`${url}?freeze=${freeze}`);
  await page.waitForFunction(() => ["ready", "error"].includes(document.body.dataset.state), null, {
    timeout: 90000,
  });
  const state = await page.locator("body").getAttribute("data-state");
  assert.equal(state, "ready", await page.locator("#status").textContent());
  report.comparison = await page.evaluate(() => window.renderLab.compare());
  report.stats = await page.evaluate(() => window.renderLab.stats());
  for (const id of ["rust", "three", "diff"]) {
    await page.locator(`#${id}`).screenshot({ path: `${output}${id}.png` });
  }
  await page.screenshot({ path: `${output}page.png` });
  // The game's default overhead pose (presentation.ts, zoom 34): shadows and PBR.
  // Three's harbor water skips its reflection when the view shows no open water
  // (`waterInView`) and keeps the last one, so compare this pose first.
  await page.evaluate(() => window.renderLab.setCamera([0, 32.32, 24.48], [0, 0.7, 0]));
  report.overhead = await page.evaluate(() => window.renderLab.compare());
  for (const id of ["rust", "three", "diff"]) {
    await page.locator(`#${id}`).screenshot({ path: `${output}overhead-${id}.png` });
  }
  // A close-up of the reflection, fog and effects from lower down.
  await page.evaluate(() => window.renderLab.setCamera([-3, 3.2, 21], [0, 1.2, 0]));
  report.closeUp = await page.evaluate(() => window.renderLab.compare());
  for (const id of ["rust", "three", "diff"]) {
    await page.locator(`#${id}`).screenshot({ path: `${output}close-${id}.png` });
  }
  // Looking away from the scene culls nearly everything.
  await page.evaluate(() => window.renderLab.setCamera([0, 9, 30], [0, 12, 80]));
  await page.evaluate(() => window.renderLab.compare());
  report.culled = await page.evaluate(() => window.renderLab.stats());
  assert.ok(report.culled.drawCalls < report.stats.drawCalls / 2, "frustum culling drops draws");
  await page.evaluate(() => window.renderLab.setCamera([0, 9, 30], [0, 1.5, 0]));
  // Resizing replaces the attachments; the image stays matched.
  await page.evaluate(() => window.renderLab.resize(800, 500));
  report.resized = await page.evaluate(() => window.renderLab.compare());
  await page.locator("#rust").screenshot({ path: `${output}resized-rust.png` });
  await page.evaluate(() => window.renderLab.resize(640, 400));
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
    `PASS: mean |Δ| ${report.comparison.meanAbsDiff.toFixed(2)} (close-up ${report.closeUp.meanAbsDiff.toFixed(2)}), ` +
      `overhead ${report.overhead.meanAbsDiff.toFixed(2)}, resized ${report.resized.meanAbsDiff.toFixed(2)}; ` +
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
