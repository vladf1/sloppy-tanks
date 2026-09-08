import { chromium } from "playwright";
import { writeFileSync } from "node:fs";
import assert from "node:assert/strict";
const browser = await chromium.launch({
  channel: "chrome",
  headless: false,
  args: ["--window-size=1600,1000"],
});
try {
  const page = await browser.newPage({
    viewport: { width: 1600, height: 1000 },
    deviceScaleFactor: 1,
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (e) => {
    if (e.type() === "error") errors.push(e.text());
  });
  await page.goto("http://127.0.0.1:5173/");
  await page.waitForFunction(() => !!window.sloppy);
  const initialBodies = await page.evaluate(() =>
    window.sloppy.sim.world.bodies.len(),
  );
  await page.screenshot({ path: "artifacts/redesign-menu.png" });
  assert.equal(await page.locator("#deploy").count(), 0);
  await page.locator('[data-kind="balanced"]').click();
  await page.waitForTimeout(200);
  await page.screenshot({ path: "artifacts/redesign-spawn.png" });
  const before = await page.evaluate(() => ({
    ...window.sloppy.sim.human.body.translation(),
  }));
  await page.mouse.move(800, 350);
  await page.keyboard.down("w");
  await page.mouse.down();
  await page.waitForTimeout(1800);
  await page.keyboard.up("w");
  await page.mouse.up();
  const driven = await page.evaluate(() => ({
    pos: window.sloppy.sim.human.body.translation(),
    shots: window.sloppy.sim.shotsFired,
  }));
  assert.ok(Math.hypot(driven.pos.x - before.x, driven.pos.z - before.z) > 2);
  assert.ok(driven.shots > 0);
  await page.mouse.move(1100, 470);
  await page.screenshot({ path: "artifacts/redesign-aim.png" });
  await page.keyboard.press("Escape");
  assert.equal(
    await page.evaluate(() => window.sloppy.sim.match.phase),
    "paused",
  );
  await page.locator("#resume").click();
  await page.evaluate(() => {
    const d = window.sloppy;
    d.sim.match.phase = "paused";
    d.sim.human.body.setTranslation({ x: 27, y: 0.6, z: 28 }, true);
    d.sim.human.previous = { x: 27, z: 28 };
    d.view.zoom = 43;
  });
  await page.waitForTimeout(100);
  await page.locator("#overlay").evaluate((e) => (e.style.display = "none"));
  await page.screenshot({ path: "artifacts/pine-village.png" });
  // Deterministic scene exercises every tank and projectile model in the production renderer.
  await page.evaluate(() => {
    const d = window.sloppy,
      s = d.sim;
    s.match.phase = "paused";
    for (const c of s.covers) c.alive = false;
    for (const p of s.pickups) p.available = false;
    for (const tank of s.tanks) {
      tank.body.setTranslation({ x: 55, y: 0.65, z: 55 }, true);
      tank.previous = { x: 55, z: 55 };
    }
    const show = [s.human, ...s.tanks.filter((t) => !t.human).slice(0, 2)];
    ["balanced", "scout", "heavy"].forEach((kind, i) => {
      const t = show[i];
      t.kind = kind;
      t.hp = kind === "heavy" ? 140 : kind === "scout" ? 80 : 100;
      t.alive = true;
      const x = [0, -6, 6][i],
        z = 0;
      t.body.setTranslation({ x, y: 0.65, z }, true);
      t.previous = { x, z };
      t.heading = 0.35;
      t.aim = 0.2;
      t.protection = 0;
    });
    s.shots = [];
    ["standard", "spread", "rocket"].forEach((weapon, i) =>
      s.shots.push({
        id: s.nextId++,
        x: -6 + i * 3,
        z: 5,
        vx: 0,
        vz: 1,
        owner: s.human.id,
        team: i % 2,
        weapon,
        life: 2,
        damage: 40,
        bounces: 0,
      }),
    );
    d.view.follow.set(0, 0, 0);
    d.view.zoom = 25;
    for (const [i, shape] of ["armor", "wheel", "track", "shard"].entries()) {
      s.fragment(-4 + i * 2, -4, 0x327fbb, 0.8, shape);
      s.fragments
        .at(-1)
        .body.setTranslation({ x: -4 + i * 2, y: 0.3, z: -4 }, true);
    }
  });
  await page.waitForTimeout(200);
  await page.locator("#overlay").evaluate((e) => (e.style.display = "none"));
  await page.screenshot({ path: "artifacts/redesign-models.png" });
  const scene = await page.evaluate(() => {
    const d = window.sloppy;
    return {
      shells: Object.values(d.view.projectiles.batches).reduce((n, b) => n + b.body.count, 0),
      cores: Object.values(d.view.projectiles.batches).reduce((n, b) => n + b.team.count, 0),
      marker: d.view.playerRing.visible,
      parts: [...d.view.debrisMeshes].map(([shape, m]) => ({
        shape,
        count: m.count,
      })),
      geometry: d.view.renderer.info.memory.geometries,
      shotColors: d.sim.shots.map((shot, i) => {
        const color = d.view.debrisColor.clone();
        const index = d.sim.shots.slice(0, i).filter(s => s.weapon === shot.weapon).length;
        d.view.projectiles.batches[shot.weapon].team.getColorAt(index, color);
        return { team: shot.team, color: color.getHex() };
      }),
    };
  });
  assert.equal(scene.shells, 5);
  assert.ok(
    scene.shotColors.every(
      (s) => s.color === (s.team === 0 ? 0x008cff : 0xff303e),
    ),
  );
  assert.equal(scene.cores, 5);
  assert.ok(scene.marker);
  assert.ok(scene.parts.every((p) => p.count > 0));
  const centering = await page.evaluate(() => {
    const { sim, view } = window.sloppy;
    return [
      [0, 0],
      [-57, -57],
      [57, -57],
      [-57, 57],
      [57, 57],
    ].map(([x, z]) => {
      sim.human.body.setTranslation({ x, y: 0.6, z }, true);
      sim.human.previous = { x: x - 1, z: z - 1 };
      view.render(sim, 0.5, 0);
      const projected = view.camera.position
        .clone()
        .set(x - 0.5, 0.7, z - 0.5)
        .project(view.camera);
      return { x, z, screenX: projected.x, screenY: projected.y };
    });
  });
  assert.ok(
    centering.every(
      (p) => Math.abs(p.screenX) < 1e-6 && Math.abs(p.screenY) < 1e-6,
    ),
  );
  const resets = [];
  for (let i = 0; i < 5; i++) {
    await page.evaluate(() => {
      window.sloppy.start();
      window.sloppy.sim.match.phase = "paused";
    });
    await page.waitForTimeout(100);
    resets.push(
      await page.evaluate(() => ({
        bodies: window.sloppy.sim.world.bodies.len(),
        geometries: window.sloppy.view.renderer.info.memory.geometries,
      })),
    );
  }
  assert.ok(resets.every((r) => r.bodies === initialBodies));
  assert.ok(resets.every((r) => r.geometries === resets[0].geometries));
  assert.deepEqual(errors, []);
  const result = {
    date: new Date().toISOString(),
    before,
    driven,
    scene,
    centering,
    resets,
    errors,
  };
  writeFileSync(
    "artifacts/redesign-check.json",
    JSON.stringify(result, null, 2),
  );
  console.log(JSON.stringify(result));
} finally {
  await browser.close();
}
