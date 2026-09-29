// Desktop play through real input: keyboard driving, mouse fire, tank choice, pause and
// zoom; then renderer resources across tower collapses, destructive resets and mines.
import { gameUrl, launchGame, startRound } from "./browser-helpers.mjs";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
const output = "artifacts/performance/browser-controls";
mkdirSync(output, { recursive: true });
const { browser, page, errors } = await launchGame();
/** Renderer allocations that must not grow; draw counts vary with the view. */
const RESOURCE_KEYS = [
  "pipelines",
  "shaderModules",
  "meshes",
  "materials",
  "textures",
  "buffers",
  "models",
  "instances",
  "gpuBytes",
];
const resources = (stats) => Object.fromEntries(RESOURCE_KEYS.map((key) => [key, stats[key]]));
try {
  // Count live WebGPU buffers at the API, independently of the renderer's own counters.
  await page.addInitScript(() => {
    const live = new Map();
    window.gpuBuffers = live;
    const create = GPUDevice.prototype.createBuffer;
    GPUDevice.prototype.createBuffer = function (descriptor) {
      const buffer = create.call(this, descriptor);
      live.set(buffer, descriptor.size);
      return buffer;
    };
    const destroy = GPUBuffer.prototype.destroy;
    GPUBuffer.prototype.destroy = function () {
      live.delete(this);
      return destroy.call(this);
    };
  });
  await page.goto(gameUrl, { waitUntil: "domcontentloaded", timeout: 60000 });
  await page.waitForFunction(() => !!window.sloppy);
  await page.screenshot({ path: `${output}/start.png` });
  await page.locator('[data-kind="balanced"]').click();
  await startRound(page);
  const before = await page.evaluate(() => window.sloppy.snapshot());
  const pose = () =>
    page.evaluate(() => {
      const { x, z, heading } = window.engine.state().human;
      return { x, z, heading };
    });
  const start = await pose();
  await page.mouse.move(1100, 440);
  await page.keyboard.down("d");
  await page.mouse.down();
  await page.waitForTimeout(1400);
  await page.keyboard.up("d");
  const right = await pose();
  // D is screen-right (+X): the hull turns onto the X axis, or reverses if it faced -X.
  assert.ok(right.x - start.x > 2, `D must drive right: ${JSON.stringify({ start, right })}`);
  assert.ok(Math.abs(Math.sin(right.heading)) > 0.9, "D aligns the hull with the X axis");
  await page.keyboard.down("s");
  await page.waitForTimeout(1200);
  await page.keyboard.up("s");
  await page.mouse.up();
  const down = await pose();
  assert.ok(down.z - right.z > 1, `S must drive toward the camera: ${JSON.stringify(down)}`);
  assert.ok((await page.evaluate(() => window.sloppy.sim.shotsFired)) > 0, "held mouse fires");
  await page.mouse.click(800, 460, { button: "right" });
  const after = await page.evaluate(() => window.sloppy.snapshot());
  await page.screenshot({ path: `${output}/driving.png` });
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

  // Tower rubble arrives mid-round: it must draw with pipelines the arena already
  // warmed up (no late compile) and get visible models.
  const beforeCollapse = await page.evaluate(() => {
    window.sloppy.overview();
    window.sloppy.collapse();
    return window.engine.stats();
  });
  await page.waitForTimeout(650);
  const collapsed = await page.evaluate(() => {
    const { engine } = window;
    const rubble = engine.covers().filter((cover) => cover.kind === "rubble");
    const views = new Map(engine.view().covers.map((cover) => [cover.id, cover]));
    return {
      rubble: rubble.length,
      shown: rubble.every((cover) => views.get(cover.id)?.shown),
      stats: engine.stats(),
    };
  });
  assert.ok(collapsed.rubble > 0 && collapsed.shown, "new rubble receives visible models");
  assert.equal(collapsed.stats.latePipelines, 0, "rubble draws with warmed pipelines");
  assert.equal(collapsed.stats.pipelines, beforeCollapse.pipelines, "no new pipelines");
  await page.screenshot({ path: `${output}/collapse.png` });
  await page.waitForTimeout(5000);
  await page.screenshot({ path: `${output}/ruined.png` });

  // Ten destructive rounds: every tank destroyed, then a fresh round. Renderer
  // allocations and live WebGPU buffers must return to the same totals.
  const resets = await page.evaluate(() => {
    const { sloppy, engine } = window;
    const game = sloppy.game;
    const memory = [];
    for (let i = 0; i < 10; i++) {
      game.debug_configure(207, 12, 0);
      sloppy.start();
      for (const tank of engine.state().tanks) {
        engine.setTank(tank.id, { protection: 0, shield: 0 });
        game.debug_damage_tank(tank.id, 999, tank.id, tank.team);
      }
      engine.draw();
      sloppy.start();
      engine.draw();
      const live = [...window.gpuBuffers.values()];
      memory.push({
        ...engine.stats(),
        gpuBuffers: live.length,
        gpuBufferBytes: live.reduce((sum, size) => sum + size, 0),
      });
    }
    return memory;
  });
  const stable = (memory) => ({
    ...resources(memory),
    gpuBuffers: memory.gpuBuffers,
    gpuBufferBytes: memory.gpuBufferBytes,
  });
  for (const memory of resets.slice(1)) {
    assert.deepEqual(stable(memory), stable(resets[0]), "resets keep renderer resources");
  }
  // Mines laid and cleared thirty times: their views must not accumulate.
  const mineResources = await page.evaluate(() => {
    const { sloppy, engine } = window;
    const game = sloppy.game;
    const { x, z, team } = engine.state().human;
    const memory = [];
    for (let i = 0; i < 30; i++) {
      game.debug_add_mine(x, z, team, 0);
      engine.draw();
      if (engine.view().mines !== 1) throw new Error("the mine needs a view");
      game.debug_clear_mines();
      engine.draw();
      memory.push({ ...engine.stats(), views: engine.view().mines });
    }
    return memory;
  });
  for (const memory of mineResources.slice(1)) {
    assert.deepEqual(resources(memory), resources(mineResources[0]), "mines keep resources");
    assert.equal(memory.views, 0);
  }
  assert.equal(await page.evaluate(() => window.sloppy.error()), null, "no GPU error");
  assert.deepEqual(errors, []);
  writeFileSync(
    `${output}/results.json`,
    JSON.stringify({ before, after, paused, resets, mineResources, errors }, null, 2),
  );
  console.log(
    JSON.stringify({
      before: before.counts,
      after: after.counts,
      paused,
      resets: stable(resets.at(-1)),
      mineResources: resources(mineResources.at(-1)),
      errors,
    }),
  );
} finally {
  await browser.close();
}
