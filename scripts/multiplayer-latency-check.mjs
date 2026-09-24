import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { seedGame, startRound, headless } from "./browser-helpers.mjs";

const directory = "artifacts/performance/multiplayer";
await mkdir(directory, { recursive: true });
const base = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";
const browser = await chromium.launch({ channel: "chrome", headless });
const cases = [];
const errors = [];
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  await seedGame(page, 4242);
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  for (const policy of ["latest", "smooth", "extrapolate"]) {
    for (const rtt of [0, 50, 100, 150]) {
      await page.goto(`${base}?latency=${rtt}&hull=${policy}`);
      await startRound(page);
      assert.equal(await page.evaluate(() => window.sloppy.sim.seed), 4242);
      await page.waitForFunction(() => window.sloppy.latency?.state.match.phase === "playing");
      await page.keyboard.down("w");
      await page.waitForTimeout(450);
      await page.keyboard.up("w");
      await page.waitForTimeout(300);
      await page.keyboard.down("d");
      await page.waitForTimeout(450);
      await page.keyboard.up("d");
      await page.mouse.move(640, 350);
      await page.mouse.down();
      await page.waitForTimeout(900);
      await page.mouse.up();
      const result = await page.evaluate(() => {
        const { latency, sim, view } = window.sloppy;
        const tank = view.tankMeshes.get(sim.human.id);
        return {
          measurements: latency.measurements,
          phase: sim.match.phase,
          physics: sim.human.body.translation(),
          display: latency.state.viewer.position,
          model: { x: tank.position.x, z: tank.position.z },
          tick: latency.state.elapsed,
          rawElapsed: sim.elapsed,
        };
      });
      assert.equal(result.phase, "playing");
      assert.ok(result.measurements.some((measurement) => measurement.type === "movement"));
      assert.ok(result.measurements.some((measurement) => measurement.type === "shot"));
      assert.ok(Math.abs(result.model.x - result.display.x) < 0.01);
      assert.ok(Math.abs(result.model.z - result.display.z) < 0.01);
      assert.ok(result.rawElapsed >= result.tick, "Presentation never runs ahead of authority");
      cases.push({ policy, rtt, ...result });
      console.log(JSON.stringify({ policy, rtt, measurements: result.measurements }));
      if (rtt === 100 && policy === "smooth") {
        await page.screenshot({ path: `${directory}/latency-100ms-smooth.png` });
      }
    }
  }
  await page.goto(`${base}?latency=100&jitter=30&inputHz=30&hull=extrapolate`);
  await startRound(page);
  await page.keyboard.down("w");
  await page.waitForTimeout(1000);
  await page.keyboard.up("w");
  await page.locator("#pause").click();
  await page.waitForTimeout(350);
  await page.locator("#resume").click();
  await page.waitForTimeout(500);
  assert.equal(await page.evaluate(() => window.sloppy.sim.match.phase), "playing");
  await page.evaluate(() => window.sloppy.latency.stall(350));
  await page.waitForFunction(() => window.sloppy.sim.match.phase === "paused");
  assert.match(await page.locator("#latency-diagnostics").innerText(), /tick debt/);
  assert.deepEqual(errors, []);
} finally {
  await browser.close();
  await writeFile(
    `${directory}/latency-cases.json`,
    JSON.stringify(
      {
        cases,
        errors,
        interpretation:
          "Automated timing and lifecycle checks; subjective playability remains a separate gate. Hit-feedback measures authoritative impact to visible feedback, excluding projectile flight.",
      },
      null,
      2,
    ),
  );
}
