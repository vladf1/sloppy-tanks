import { gameUrl, launchGame, startRound } from "./browser-helpers.mjs";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
const output = "artifacts/performance/browser-controls";
mkdirSync(output, { recursive: true });
const { browser, page, errors } = await launchGame();
try {
  await page.goto(gameUrl, { waitUntil: "domcontentloaded", timeout: 60000 });
  await page.waitForFunction(() => !!window.sloppy);
  await page.screenshot({ path: `${output}/start.png` });
  await page.locator('[data-kind="balanced"]').click();
  await startRound(page);
  const before = await page.evaluate(() => window.sloppy.sim.snapshot());
  const pose = () =>
    page.evaluate(() => {
      const tank = window.sloppy.sim.human;
      return { ...tank.body.translation(), heading: tank.heading };
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
  await page.mouse.click(800, 460, { button: "right" });
  const after = await page.evaluate(() => window.sloppy.sim.snapshot());
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
  await page.screenshot({ path: `${output}/collapse.png` });
  await page.waitForTimeout(5000);
  await page.screenshot({ path: `${output}/ruined.png` });
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
  writeFileSync(
    `${output}/results.json`,
    JSON.stringify({ before, after, paused, resets, mineResources, errors }, null, 2),
  );
  console.log(
    JSON.stringify({
      before: before.tanks.find((t) => t.personality === "player"),
      after: after.counts,
      paused,
      resets: resets.at(-1),
      mineResources: mineResources.at(-1),
      errors,
    }),
  );
} finally {
  await browser.close();
}
