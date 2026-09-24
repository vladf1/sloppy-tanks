import { chromium } from "playwright";
import { headless } from "./browser-helpers.mjs";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";
const browser = await chromium.launch({
  channel: "chrome",
  headless,
  args: ["--window-size=1600,1000"],
});
try {
  const context = await browser.newContext({
    viewport: { width: 1600, height: 900 },
    deviceScaleFactor: 1,
  });
  const errors = [];
  const inputPage = await context.newPage();
  inputPage.on("pageerror", (e) => errors.push(e.message));
  // Drive the actual application loop between physics ticks, independent of display Hz.
  await inputPage.addInitScript(() => {
    let frame, now;
    const requestFrame = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (callback) => {
      if (callback.name !== "loop") return requestFrame(callback);
      frame = callback;
      if (!window.sloppy) return requestFrame(callback);
      return 1;
    };
    window.advanceFrame = (ms) => {
      now = (now ?? performance.now()) + ms;
      frame(now);
    };
  });
  await inputPage.goto(url);
  await inputPage.waitForFunction(() => !!window.sloppy);
  const input = await inputPage.evaluate(() => {
    const d = window.sloppy;
    d.start();
    window.advanceFrame(100);
    const commands = [],
      step = d.sim.step.bind(d.sim);
    d.sim.step = (command, autoplay) => {
      commands.push(command.mine);
      step(command, autoplay);
    };
    document.querySelector("#game").dispatchEvent(new PointerEvent("pointerdown", { button: 2 }));
    window.advanceFrame(4);
    const betweenSteps = {
      pending: d.controls.mine,
      steps: commands.length,
      mines: d.sim.mines.length,
    };
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
  await page.locator("#start").click();
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
  await page.keyboard.down("Shift");
  await page.mouse.wheel(0, 100);
  await page.keyboard.up("Shift");
  await page.waitForFunction((value) => window.sloppy.view.zoom !== value, zoom);
  const beforeCollapseBatches = await page.evaluate(() => {
    const batches = window.sloppy.view.partBatches.batches.map((batch) => batch.mesh.uuid);
    window.sloppy.overview();
    window.sloppy.collapse();
    return batches;
  });
  await page.waitForTimeout(650);
  assert.deepEqual(
    await page.evaluate(() =>
      window.sloppy.view.partBatches.batches.map((batch) => batch.mesh.uuid),
    ),
    beforeCollapseBatches,
    "tower rubble must not rebuild the warmed tank/tree batches",
  );
  assert.ok(
    await page.evaluate(() => {
      const d = window.sloppy;
      const rubble = d.sim.covers.filter((cover) => cover.kind === "rubble");
      return (
        rubble.length > 0 && rubble.every((cover) => d.view.coverMeshes.get(cover.id)?.visible)
      );
    }),
    "new rubble still receives visible models",
  );
  await page.screenshot({ path: "artifacts/collapse.png" });
  await page.waitForTimeout(5000);
  await page.screenshot({ path: "artifacts/ruined.png" });
  const resets = await page.evaluate(() => {
    const d = window.sloppy,
      memory = [];
    const device = d.view.renderer.backend.device;
    const buffers = new Map();
    // Count actual live allocations as well as the renderer's resource counters.
    if (device) {
      const createBuffer = device.createBuffer.bind(device);
      device.createBuffer = (descriptor) => {
        const buffer = createBuffer(descriptor);
        buffers.set(buffer, descriptor.size);
        const destroy = buffer.destroy.bind(buffer);
        buffer.destroy = () => {
          buffers.delete(buffer);
          destroy();
        };
        return buffer;
      };
    }
    for (let i = 0; i < 10; i++) {
      d.sim.seed = 207;
      d.start();
      for (const tank of d.sim.tanks) {
        tank.protection = 0;
        d.sim.damageTank(tank, 999, tank.id, tank.team);
      }
      d.view.render(d.sim, 1, 0);
      d.start();
      d.view.render(d.sim, 1, 0);
      memory.push({
        ...d.view.renderer.info.memory,
        gpuBuffers: buffers.size,
        gpuBufferBytes: [...buffers.values()].reduce((sum, size) => sum + size, 0),
      });
    }
    return memory;
  });
  const stableMemory = ({ programsSize, total, ...memory }) => ({
    ...memory,
    // Generated shader identifiers grow with node IDs. Shader count must stay
    // fixed; compare actual buffer/texture bytes independently of source length.
    resourceBytes: total - programsSize,
  });
  for (const memory of resets.slice(1)) {
    assert.deepEqual(stableMemory(memory), stableMemory(resets[0]));
  }
  const mineResources = await page.evaluate(() => {
    const { sim, view } = window.sloppy;
    const memory = [];
    for (let i = 0; i < 30; i++) {
      sim.mines.push({
        id: sim.nextId++,
        owner: sim.human.id,
        team: sim.humanTeam,
        x: sim.human.previous.x,
        z: sim.human.previous.z,
        arm: 0,
        life: 20,
      });
      view.render(sim, 1, 0);
      sim.mines.length = 0;
      view.render(sim, 1, 0);
      memory.push({ ...view.renderer.info.memory });
    }
    return memory;
  });
  for (const memory of mineResources.slice(1)) {
    assert.deepEqual(stableMemory(memory), stableMemory(mineResources[0]));
  }
  assert.deepEqual(errors, []);
  mkdirSync("artifacts/performance", { recursive: true });
  writeFileSync(
    "artifacts/performance/browser-controls.json",
    JSON.stringify({ input, before, after, paused, resets, mineResources, errors }, null, 2),
  );
  console.log(
    JSON.stringify({
      before: before.tanks.find((t) => t.personality === "player"),
      after: after.counts,
      paused,
      input,
      resets: resets.at(-1),
      mineResources: mineResources.at(-1),
      errors,
    }),
  );
} finally {
  await browser.close();
}
