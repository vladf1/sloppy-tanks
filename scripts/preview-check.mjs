import { chromium } from "playwright";
import assert from "node:assert/strict";
const browser = await chromium.launch({ channel: "chrome", headless: false });
try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("http://127.0.0.1:5173/");
  await page.waitForFunction(
    () =>
      document.querySelectorAll(".tank-preview").length === 3 &&
      [...document.querySelectorAll(".tank-preview")].every(
        (i) => i.complete && i.naturalWidth === 640,
      ),
  );
  await page.screenshot({ path: "artifacts/tank-selection.png" });
  await page.locator('[data-kind="heavy"]').click();
  await page.waitForFunction(
    () => window.sloppy.sim.match.phase === "playing" && window.sloppy.sim.human.kind === "heavy",
  );
  assert.deepEqual(errors, []);
  console.log("Three actual-model previews loaded; Big Rig click starts play; no page errors.");
} finally {
  await browser.close();
}
