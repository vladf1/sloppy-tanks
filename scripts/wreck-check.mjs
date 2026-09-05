import { chromium } from "playwright";
import assert from "node:assert/strict";
import { writeFileSync } from "node:fs";
const browser = await chromium.launch({ channel: "chrome", headless: false });
try {
  const page = await browser.newPage({
      viewport: { width: 1600, height: 1000 },
    }),
    errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (e) => {
    if (e.type() === "error") errors.push(e.text());
  });
  await page.goto("http://127.0.0.1:5173/");
  await page.waitForFunction(() => !!window.sloppy);
  const speeds = await page.locator(".spec").allTextContents();
  assert.ok(
    speeds.some((s) => s.includes("35 KM/H")) &&
      speeds.some((s) => s.includes("28 KM/H")) &&
      speeds.some((s) => s.includes("23 KM/H")),
  );
  const choices = [];
  for (const kind of ["scout", "balanced", "heavy"]) {
    await page.locator(`[data-kind="${kind}"]`).click();
    const state = await page.evaluate(() => ({
      kind: window.sloppy.sim.human.kind,
      phase: window.sloppy.sim.match.phase,
    }));
    assert.deepEqual(state, { kind, phase: "playing" });
    choices.push(state);
    await page.evaluate(() => window.sloppy.restart());
    await page.waitForFunction(() => window.sloppy.sim.match.phase === "ready");
  }
  await page.locator('[data-kind="balanced"]').click();
  await page.evaluate(() => {
    const { sim, view } = window.sloppy;
    // Open a controlled landing patch and leave ordinary physics/rendering running.
    for (const c of sim.covers)
      if (c.kind !== "boundary") {
        sim.world.removeRigidBody(c.body);
        c.alive = false;
      }
    sim.nav.rebuild(sim.covers);
    sim.human.body.setTranslation({ x: 0, y: 0.6, z: 0 }, true);
    sim.human.previous = { x: 0, z: 0 };
    for (const t of sim.tanks) {
      t.cooldown = 100;
      t.protection = 100;
    }
    view.render(sim, 1, 0);
    sim.human.protection = 0;
    sim.damageTank(sim.human, 1000, 999, 1 - sim.humanTeam);
    window.wreckIds = sim.fragments.map((f) => f.id);
  });
  await page.waitForTimeout(650);
  await page.screenshot({ path: "artifacts/wreck-flight.png" });
  const flight = await page.evaluate(() =>
    window.sloppy.sim.fragments
      .filter((f) => window.wreckIds.includes(f.id))
      .map((f) => ({
        part: f.part,
        pos: f.body.translation(),
        spin: f.body.angvel(),
      })),
  );
  await page.waitForTimeout(1950);
  await page.screenshot({ path: "artifacts/wreck-landed.png" });
  const landed = await page.evaluate(() => {
    const d = window.sloppy;
    return d.sim.fragments
      .filter((f) => window.wreckIds.includes(f.id))
      .map((f) => {
        const p = f.body.translation(),
          ndc = d.view.camera.position
            .clone()
            .set(p.x, p.y, p.z)
            .project(d.view.camera);
        return { part: f.part, pos: p, screen: { x: ndc.x, y: ndc.y } };
      });
  });
  assert.ok(flight.length >= 2 && flight.some((f) => f.pos.y > 3));
  assert.ok(
    landed.every(
      (f) =>
        f.pos.y < 2 && Math.abs(f.screen.x) < 1 && Math.abs(f.screen.y) < 1,
    ),
  );
  await page.waitForTimeout(2000);
  const remaining = await page.evaluate(
    () =>
      window.sloppy.sim.fragments.filter((f) => window.wreckIds.includes(f.id))
        .length,
  );
  assert.equal(remaining, 0);
  assert.deepEqual(errors, []);
  const result = { speeds, choices, flight, landed, remaining, errors };
  writeFileSync("artifacts/wreck-check.json", JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result));
} finally {
  await browser.close();
}
