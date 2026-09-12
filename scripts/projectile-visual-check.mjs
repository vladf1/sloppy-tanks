import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5179/sloppy-tanks/";
const out = "artifacts/performance/projectiles";
mkdirSync(out, { recursive: true });
const browser = await chromium.launch({
  channel: "chrome",
  headless: false,
  args: ["--disable-backgrounding-occluded-windows", "--disable-renderer-backgrounding"],
});
const errors = [];
try {
  const context = await browser.newContext({
    viewport: { width: 1600, height: 900 },
    deviceScaleFactor: 1,
  });
  context.on("page", (p) => p.on("pageerror", (e) => errors.push(e.message)));
  const page = await context.newPage();
  await page.addInitScript(() => {
    let frame, now;
    window.requestAnimationFrame = (cb) => {
      frame = cb;
      return 1;
    };
    window.advanceFrame = (ms) => {
      now = (now ?? performance.now()) + ms;
      frame(now);
    };
  });
  await page.goto(url);
  await page.waitForFunction(() => !!window.sloppy);
  const shapes = await page.evaluate(() => {
    const d = window.sloppy;
    d.start();
    d.autoplay(false);
    const s = d.sim,
      t = s.human;
    for (const bot of s.tanks) if (bot !== t) s.world.removeRigidBody(bot.body);
    s.tanks = [t];
    s.pickups = [];
    s.shots = [];
    s.events = [];
    for (const c of s.covers) s.world.removeRigidBody(c.body);
    s.covers = [];
    s.coverByCollider.clear();
    s.nav.rebuild([]);
    t.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    t.previous = { x: 0, z: 0 };
    d.view.reset(s);
    d.view.zoom = 23;
    const weapons = ["standard", "spread", "rocket", "ricochet", "piercing"];
    for (const team of [0, 1])
      for (const [i, weapon] of weapons.entries()) {
        const count = weapon === "spread" ? 3 : 1;
        for (let pellet = 0; pellet < count; pellet++)
          s.shots.push({
            id: s.nextId++,
            weapon,
            team,
            owner: t.id,
            x: (i - 2) * 4 + (pellet - (count - 1) / 2) * 0.4,
            z: team * 4 - 5,
            y: 1,
            vx: 12,
            vz: -18,
            life: 3.5,
            damage: 0,
            bounces: 0,
            piercing: 0,
          });
      }
    d.view.render(s, 1, 0);
    d.view.playerRing.visible = false;
    d.view.spawnPulse.visible = false;
    d.view.crosshair.visible = false;
    for (const g of d.view.tankMeshes.values()) g.visible = false;
    for (const g of d.view.bars.values()) g.visible = false;
    d.view.renderer.render(d.view.scene, d.view.camera);
    document.querySelector("#hud").style.display = "none";
    document.querySelector("#overlay").style.display = "none";
    // Labels belong only to this isolated comparison fixture.
    const labels = document.createElement("div");
    labels.id = "comparison-labels";
    labels.style.cssText =
      "position:fixed;inset:0;pointer-events:none;font:bold 16px system-ui;color:white;text-shadow:0 1px 4px black";
    for (const [i, weapon] of weapons.entries()) {
      const p = d.view.follow
        .clone()
        .set((i - 2) * 4, 1, -7)
        .project(d.view.camera);
      const label = document.createElement("span");
      label.textContent = weapon.toUpperCase();
      label.style.cssText = `position:absolute;left:${(p.x + 1) * 50}%;top:${(1 - p.y) * 50}%;transform:translate(-50%,-100%)`;
      labels.append(label);
    }
    document.body.append(labels);
    const original = JSON.stringify(s.shots);
    d.view.projectiles.update(s.shots, 2);
    if (JSON.stringify(s.shots) !== original)
      throw new Error("visual update mutated simulation shots");
    return Object.entries(d.view.projectiles.batches).map(([weapon, b]) => {
      b.body.geometry.computeBoundingBox();
      const bounds = b.body.geometry.boundingBox;
      return {
        weapon,
        count: b.body.count,
        layers: b.exhaust ? 3 : 2,
        width: bounds.max.x - bounds.min.x,
        length: bounds.max.z - bounds.min.z,
        triangles: [b.body, b.team, b.exhaust]
          .filter(Boolean)
          .reduce(
            (n, m) => n + (m.geometry.index?.count ?? m.geometry.attributes.position.count) / 3,
            0,
          ),
      };
    });
  });
  assert.deepEqual(
    shapes.map((s) => s.count),
    [2, 6, 2, 2, 2],
  );
  assert.ok(
    shapes.every((s) => s.width <= 0.53 && s.length <= 1.01),
    "keep projectiles compact",
  );
  await page.screenshot({ path: `${out}/all-munitions.png` });
  await page.screenshot({
    path: `${out}/comparison.png`,
    clip: { x: 430, y: 205, width: 750, height: 255 },
  });
  await page.evaluate(() => {
    const d = window.sloppy;
    // Bring only the models closer for the shape/detail inspection.
    d.view.camera.position.set(0, 10, 4);
    d.view.camera.lookAt(0, 0, -3);
    d.view.camera.updateMatrixWorld();
    document.querySelector("#comparison-labels").remove();
    d.view.renderer.render(d.view.scene, d.view.camera);
  });
  await page.screenshot({ path: `${out}/close-up.png` });
  const buffers = await page.evaluate(() => {
    const d = window.sloppy,
      v = d.view.projectiles,
      template = d.sim.shots;
    const shots = Array.from({ length: 650 }, (_, i) => ({
      ...template[i % template.length],
      id: i,
      x: ((i % 30) - 15) * 1.5,
      z: (Math.floor(i / 30) - 10) * 1.5,
    }));
    v.update(shots, 3);
    const count = Object.values(v.batches).reduce((n, b) => n + b.body.count, 0);
    for (const b of Object.values(v.batches)) {
      if (b.body.count !== b.team.count || (b.exhaust && b.exhaust.count !== b.body.count))
        throw new Error("misaligned layers");
      for (const layer of [b.body, b.team, b.exhaust].filter(Boolean)) {
        const used = layer.instanceMatrix.array.subarray(0, layer.count * 16);
        if (![...used].every(Number.isFinite)) throw new Error("invalid transform");
      }
    }
    d.view.renderer.render(d.view.scene, d.view.camera);
    v.update([], 4);
    if (v.group.children.some((m) => m.count !== 0)) throw new Error("stale projectile instances");
    const geometry = [];
    for (let i = 0; i < 10; i++) {
      d.restart();
      d.view.render(d.sim, 1, 0);
      geometry.push(d.view.renderer.info.memory.geometries);
    }
    return { count, geometry };
  });
  assert.equal(buffers.count, 600);
  assert.equal(new Set(buffers.geometry).size, 1);
  await page.close();
  const live = await context.newPage();
  await live.goto(`${url}?autoplay`);
  await live.waitForFunction(() => !!window.sloppy);
  await live.evaluate(() => {
    const d = window.sloppy;
    d.sim.seed = 12345;
    d.sim.roundCount = 24;
    d.sim.mapMode = "surprise";
    d.start();
    d.autoplay();
    const step = d.sim.step.bind(d.sim);
    d.sim.step = (...args) => {
      for (const t of d.sim.tanks) {
        t.ammo = { spread: 36, rocket: 24, ricochet: 48, piercing: 48 };
        t.rapid = 12;
      }
      step(...args);
    };
    d.record();
  });
  await live.waitForTimeout(15000);
  const report = await live.evaluate(() => window.sloppy.stop());
  assert.ok(report.snapshot.elapsed > 13);
  assert.deepEqual(errors, []);
  await live.screenshot({ path: `${out}/live-combat.png` });
  writeFileSync(
    "artifacts/projectile-visual-results.json",
    JSON.stringify(
      {
        date: new Date().toISOString(),
        chrome: browser.version(),
        shapes,
        buffers,
        report,
        errors,
      },
      null,
      2,
    ),
  );
  console.log(
    JSON.stringify(
      {
        shapes,
        buffers,
        fps: report.fps,
        p99: report.frameP99,
        simulationMean: report.simulationMean,
        renderMean: report.renderMean,
        maxProjectiles: report.maxProjectiles,
        errors,
      },
      null,
      2,
    ),
  );
} finally {
  await browser.close();
}
