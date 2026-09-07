import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";
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
  const errors = [];
  const inputPage = await context.newPage();
  inputPage.on("pageerror", e => errors.push(e.message));
  // Drive the actual application loop between physics ticks, independent of display Hz.
  await inputPage.addInitScript(() => {
    let frame, now;
    window.requestAnimationFrame = callback => { frame = callback; return 1; };
    window.advanceFrame = ms => { now = (now ?? performance.now()) + ms; frame(now); };
  });
  await inputPage.goto(url);
  await inputPage.waitForFunction(() => !!window.sloppy);
  const input = await inputPage.evaluate(() => {
    const d = window.sloppy;
    d.start(); window.advanceFrame(100);
    const commands = [], step = d.sim.step.bind(d.sim);
    d.sim.step = (command, autoplay) => { commands.push(command.mine); step(command, autoplay); };
    document.querySelector("#game").dispatchEvent(new PointerEvent("pointerdown", { button: 2 }));
    window.advanceFrame(4);
    const betweenSteps = { pending: d.controls.mine, steps: commands.length, mines: d.sim.mines.length };
    window.advanceFrame(16);
    window.advanceFrame(40);
    return { betweenSteps, commands, mines: d.sim.mines.length };
  });
  assert.deepEqual(input.betweenSteps, { pending: true, steps: 0, mines: 0 });
  assert.equal(input.commands.filter(Boolean).length, 1);
  assert.equal(input.commands[0], true);
  assert.ok(input.commands.length >= 3, "exercise multiple catch-up steps");
  assert.equal(input.mines, 1);
  await inputPage.close();
  const page = await context.newPage();
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(url, {
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
  assert.equal(paused, "paused");
  await page.locator("#resume").click();
  await page.waitForFunction(() => document.querySelector("#overlay").style.display === "none");
  const zoom = await page.evaluate(() => window.sloppy.view.zoom);
  await page.mouse.wheel(0, 100);
  await page.waitForFunction(value => window.sloppy.view.zoom !== value, zoom);
  await page.evaluate(() => {
    window.sloppy.overview();
    window.sloppy.collapse();
  });
  await page.waitForTimeout(650);
  await page.screenshot({ path: "artifacts/collapse.png" });
  await page.waitForTimeout(5000);
  await page.screenshot({ path: "artifacts/ruined.png" });
  const resets = await page.evaluate(() => {
    const d = window.sloppy, memory = [];
    for (let i = 0; i < 10; i++) {
      d.sim.seed = 207; d.start();
      for (const tank of d.sim.tanks) {
        tank.protection = 0; d.sim.damageTank(tank, 999, tank.id, tank.team);
      }
      d.view.render(d.sim, 1, 0);
      d.start(); d.view.render(d.sim, 1, 0);
      memory.push({ ...d.view.renderer.info.memory });
    }
    return memory;
  });
  for (const memory of resets.slice(1)) assert.deepEqual(memory, resets[0]);
  assert.deepEqual(errors, []);
  mkdirSync("artifacts/performance", { recursive: true });
  writeFileSync(
    "artifacts/performance/browser-controls.json",
    JSON.stringify({ input, before, after, paused, resets, errors }, null, 2),
  );
  console.log(
    JSON.stringify({
      before: before.tanks.find(t => t.personality === "player"),
      after: after.counts,
      paused,
      input,
      resets: resets.at(-1),
      errors,
    }),
  );
} finally {
  await browser.close();
}
