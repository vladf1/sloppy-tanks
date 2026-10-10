// Steps shared by render-lab-check.mjs and effects-lab-check.mjs. Both lab pages set
// `body[data-state]` to ready or error, explain it in `#status`, and draw the engine,
// the reference and the difference into `#rust`, `#reference` and `#diff`.
import assert from "node:assert/strict";
import { launchGame } from "../scripts/browser-helpers.mjs";

/** Headless Chrome wide enough for the three panels, collecting console errors. */
export function launchLab() {
  return launchGame({ viewport: { width: 2000, height: 1100 }, consoleErrors: true });
}

/** Open a lab page and wait until it has prepared; it must be ready. */
export async function openLab(page, url) {
  await page.goto(url);
  await page.waitForFunction(() => ["ready", "error"].includes(document.body.dataset.state), null, {
    timeout: 90000,
  });
  const state = await page.locator("body").getAttribute("data-state");
  assert.equal(state, "ready", await page.locator("#status").textContent());
}

/** Screenshot the three panels to `<output><name>-<panel>.png`. */
export async function shoot(page, output, name) {
  for (const id of ["rust", "reference", "diff"]) {
    await page.locator(`#${id}`).screenshot({ path: `${output}${name}-${id}.png` });
  }
}

/** Against a reference, the mean error must stay within `tolerance` (of 255). */
export function within(comparison, name, tolerance) {
  if (comparison.reference) {
    assert.ok(
      comparison.meanAbsDiff <= tolerance,
      `${name}: mean |Δ| ${comparison.meanAbsDiff} exceeds ${tolerance} against the reference`,
    );
  }
}

/** A comparison's mean error for the summary line. */
export function meanError(comparison) {
  return comparison.reference ? comparison.meanAbsDiff.toFixed(2) : "no reference";
}
