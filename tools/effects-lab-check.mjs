// Headless check of the effects lab: startup and warm-up, the frozen side-by-side
// scene against the captured Three.js references (when present), repeated triggers
// (pools stay bounded, no late pipelines), reset and
// GPU/console errors. Needs the labs engine (`pnpm run wasm:labs`) and a dev server:
//   pnpm exec vite --host 127.0.0.1 --port 5194 --strictPort
//   EFFECTS_LAB_URL=http://127.0.0.1:5194/sloppy-tanks/tools/effects-lab.html node tools/effects-lab-check.mjs
// Screenshots and the report go to artifacts/performance/effects-lab/.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { launchGame } from "../scripts/browser-helpers.mjs";

const url =
  process.env.EFFECTS_LAB_URL ?? "http://127.0.0.1:5194/sloppy-tanks/tools/effects-lab.html";
const output = fileURLToPath(new URL("../artifacts/performance/effects-lab/", import.meta.url));
await mkdir(output, { recursive: true });
const { browser, page, errors } = await launchGame({
  viewport: { width: 2000, height: 1100 },
  consoleErrors: true,
});
const report = {};
/** The effects' cosmetic randomness differs from the Three side's, so the references
 * match only overall: the last calibration measured 1.9–2.1 (4.2 in the close-up). */
const tolerance = Number(process.env.EFFECTS_LAB_TOLERANCE ?? 4);
const within = (comparison, name) => {
  if (comparison.reference) {
    assert.ok(
      comparison.meanAbsDiff <= tolerance,
      `${name}: mean |Δ| ${comparison.meanAbsDiff} exceeds ${tolerance} against the reference`,
    );
  }
};
const shoot = async (name) => {
  for (const id of ["rust", "reference", "diff"]) {
    await page.locator(`#${id}`).screenshot({ path: `${output}${name}-${id}.png` });
  }
};
try {
  for (const [name, query] of [
    ["village", "?t=4"],
    ["quarry", "?t=4&theme=quarry"],
  ]) {
    await page.goto(`${url}${query}`);
    await page.waitForFunction(
      () => ["ready", "error"].includes(document.body.dataset.state),
      null,
      { timeout: 90000 },
    );
    const state = await page.locator("body").getAttribute("data-state");
    assert.equal(state, "ready", await page.locator("#status").textContent());
    report[name] = {
      comparison: await page.evaluate(() => window.effectsLab.compare()),
      stats: await page.evaluate(() => window.effectsLab.stats()),
    };
    await shoot(name);
    await page.screenshot({ path: `${output}${name}-page.png` });
    assert.equal(report[name].stats.latePipelines, 0, "warm-up compiled every effect");
    within(report[name].comparison, name);
  }
  // A close view of the blasts.
  report.close = await page.evaluate(() => window.effectsLab.closeUp());
  await shoot("close");
  // Keep the script running through several periods: pools stay bounded.
  report.long = await page.evaluate(() => window.effectsLab.advance(20));
  const capacity = report.long.poolList.length;
  assert.ok(capacity > 20, "every effect pool is registered");
  assert.equal(report.long.latePipelines, 0, "no pipeline compiled mid-run");
  // A flood of blasts saturates the particle and puff pools without errors.
  report.flood = await page.evaluate(() => {
    for (let i = 0; i < 400; i++) {
      window.effectsLab.trigger({ type: "explosion", x: (i % 20) - 10, z: 5, size: 5 });
      window.effectsLab.trigger({ type: "death", x: (i % 20) - 10, z: -5, size: 3 });
    }
    return window.effectsLab.advance(0.2);
  });
  assert.ok(report.flood.effects.particles <= 1200);
  assert.ok(report.flood.effects.puffs <= 192);
  await shoot("flood");
  // Reset clears every transient pool; only shots still in flight (with the smoke
  // trailing their rockets) and the laser tank's lens redraw, and empty pools add no
  // draws.
  await page.evaluate(() => window.effectsLab.setCamera([0, 21, 25], [0, 0, 0.5]));
  report.reset = await page.evaluate(() => {
    window.effectsLab.reset();
    window.effectsLab.compare();
    return window.effectsLab.stats();
  });
  const cleared = report.reset.effects;
  for (const key of ["particles", "blasts", "puffs", "trackMarks", "pickupEffects", "laserBeams"]) {
    assert.equal(cleared[key], 0, `${key} after reset`);
  }
  const live = report.reset.poolList.filter(([, count]) => count > 0).map(([label]) => label);
  assert.ok(
    live.every((label) => /projectile|rocket smoke|laser (lenses|mounts)|quarry dust/.test(label)),
    `unexpected live pools after reset: ${live}`,
  );
  report.gpuError = await page.evaluate(() => window.effectsLab.error());
  assert.equal(report.gpuError, null);
  report.consoleErrors = errors;
  assert.deepEqual(errors, [], "no console errors");
} finally {
  await writeFile(`${output}report.json`, JSON.stringify(report, null, 1));
  await browser.close();
}
const error = (comparison) =>
  comparison.reference ? comparison.meanAbsDiff.toFixed(2) : "no reference";
console.log(
  `effects lab ok: mean |Δ| vs Three.js village ${error(report.village.comparison)}, quarry ${error(report.quarry.comparison)}, close-up ${error(report.close)}; ${output}`,
);
