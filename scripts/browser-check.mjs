import { chromium } from "playwright";
import { writeFileSync } from "node:fs";
const browser = await chromium.launch({
  channel: "chrome",
  headless: false,
  args: ["--window-size=1600,1000"],
});
try {
  const context = await browser.newContext({
    viewport: { width: 1600, height: 900 },
    deviceScaleFactor: 1,
  });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("http://127.0.0.1:5173", {
    waitUntil: "domcontentloaded",
    timeout: 60000,
  });
  await page.waitForFunction(() => !!window.sloppy);
  await page.screenshot({ path: "artifacts/start.png" });
  await page.locator('[data-kind="balanced"]').click();
  const before = await page.evaluate(() => window.sloppy.sim.snapshot());
  await page.mouse.move(1100, 440);
  await page.keyboard.down("d");
  await page.mouse.down();
  await page.waitForTimeout(1400);
  await page.keyboard.up("d");
  await page.keyboard.down("s");
  await page.waitForTimeout(1200);
  await page.keyboard.up("s");
  await page.mouse.up();
  await page.mouse.click(800, 460, { button: "right" });
  const after = await page.evaluate(() => window.sloppy.sim.snapshot());
  await page.screenshot({ path: "artifacts/driving.png" });
  await page.keyboard.press("Escape");
  await page.waitForTimeout(200);
  const paused = await page.evaluate(() => window.sloppy.sim.match.phase);
  await page.locator("#resume").click();
  await page.evaluate(() => {
    window.sloppy.overview();
    window.sloppy.collapse();
  });
  await page.waitForTimeout(650);
  await page.screenshot({ path: "artifacts/collapse.png" });
  await page.waitForTimeout(5000);
  await page.screenshot({ path: "artifacts/ruined.png" });
  writeFileSync(
    "artifacts/browser-controls.json",
    JSON.stringify({ before, after, paused, errors }, null, 2),
  );
  console.log(
    JSON.stringify({
      before: before.tanks.find(
        (t) => t.id === before.tanks.find((_, i) => i === 0)?.id,
      ),
      after: after.counts,
      paused,
      errors,
    }),
  );
} finally {
  await browser.close();
}
