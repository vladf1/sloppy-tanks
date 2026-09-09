import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5179/sloppy-tanks/";
const out = "artifacts/performance/bot-movement";
mkdirSync(out, { recursive: true });
const browser = await chromium.launch({
  channel: "chrome",
  headless: false,
  args: ["--disable-backgrounding-occluded-windows", "--disable-renderer-backgrounding"],
});
const errors = [],
  checks = [];
try {
  const context = await browser.newContext({
    viewport: { width: 1600, height: 900 },
    deviceScaleFactor: 1,
  });
  const page = await context.newPage();
  page.on("pageerror", (e) => errors.push(e.message));
  for (const scenario of ["retreat-at-wall", "head-on-allies"]) {
    await page.goto(url);
    await page.waitForFunction(() => !!window.sloppy);
    await page.evaluate((scenario) => {
      const d = window.sloppy;
      d.start();
      d.autoplay(false);
      const s = d.sim;
      for (const c of s.covers) s.world.removeRigidBody(c.body);
      for (const t of s.tanks) s.world.removeRigidBody(t.body);
      s.covers = [];
      s.coverByCollider.clear();
      s.tanks = [];
      s.pickups = [];
      const bot = s.addTank(0, false, "balanced", 1);
      const other = s.addTank(scenario === "head-on-allies" ? 0 : 1, false, "balanced", 1);
      const player = s.addTank(1, true, "balanced");
      s.humanTeam = 1;
      for (const [t, x, z] of [
        [bot, 0, scenario === "head-on-allies" ? -6 : -8],
        [other, 0, 6],
        [player, 0, 0],
      ]) {
        t.body.setTranslation({ x, y: 0.65, z }, true);
        t.previous = { x, z };
        t.brain.last = { x, z };
        t.protection = 1000;
      }
      if (scenario === "retreat-at-wall") {
        s.world.removeRigidBody(other.body);
        s.tanks.splice(s.tanks.indexOf(other), 1);
        s.addCover({
          kind: "concrete",
          x: 0,
          z: -12,
          w: 16,
          d: 2,
          h: 3,
          hp: Infinity,
          color: 0xc4d4da,
        });
      } else {
        // The observer renders the scene but does not obstruct the two bots.
        for (let i = 0; i < player.body.numColliders(); i++)
          player.body.collider(i).setCollisionGroups(0);
        for (const [t, z] of [
          [bot, 16],
          [other, -16],
        ]) {
          t.brain.decision = 999;
          t.brain.goal = { x: 0, z };
        }
      }
      s.nav.rebuild(s.covers);
      if (scenario === "head-on-allies")
        for (const t of [bot, other]) t.brain.path = s.nav.find(t.previous, t.brain.goal);
      s.world.step();
      d.view.reset(s);
      d.view.zoom = 29;
      const step = s.step.bind(s),
        last = new Map(),
        flips = new Map();
      window.botCheck = () =>
        s.tanks
          .filter((t) => !t.human)
          .map((t) => ({
            x: t.body.translation().x,
            z: t.body.translation().z,
            goal: t.brain.goal,
            reversals: flips.get(t.id) ?? 0,
          }));
      s.step = (...args) => {
        step(...args);
        for (const t of s.tanks.filter((t) => !t.human)) {
          const c = t.command,
            length = Math.hypot(c.moveX, c.moveZ);
          if (length < 0.15) continue;
          const v = [c.moveX / length, c.moveZ / length],
            previous = last.get(t.id);
          if (previous && previous[0] * v[0] + previous[1] * v[1] < -0.5)
            flips.set(t.id, (flips.get(t.id) ?? 0) + 1);
          last.set(t.id, v);
        }
      };
    }, scenario);
    await page.waitForTimeout(scenario === "retreat-at-wall" ? 4000 : 10000);
    const result = await page.evaluate(() => window.botCheck());
    assert.ok(result.every((t) => t.reversals < 5));
    if (scenario === "retreat-at-wall") assert.ok(Math.abs(result[0].x) > 8);
    else assert.ok(result.every((t) => Math.hypot(t.x - t.goal.x, t.z - t.goal.z) < 0.5));
    checks.push({ scenario, result });
    await page.screenshot({ path: `${out}/${scenario}.png` });
    console.log(JSON.stringify(checks.at(-1)));
  }
  await page.goto(`${url}?autoplay`);
  await page.waitForFunction(() => !!window.sloppy);
  await page.evaluate(() => {
    const d = window.sloppy;
    d.sim.seed = 12345;
    d.sim.roundCount = 24;
    d.sim.mapMode = "random";
    d.start();
    d.autoplay();
    d.record();
  });
  await page.waitForTimeout(20000);
  const report = await page.evaluate(() => window.sloppy.stop());
  assert.ok(report.snapshot.elapsed > 18);
  assert.deepEqual(errors, []);
  await page.screenshot({ path: `${out}/random-map-match.png` });
  writeFileSync(
    "artifacts/bot-movement-browser.json",
    JSON.stringify(
      {
        date: new Date().toISOString(),
        chrome: browser.version(),
        checks,
        report,
        errors,
      },
      null,
      2,
    ),
  );
  console.log(
    JSON.stringify({
      fps: report.fps,
      simulationMean: report.simulationMean,
      frameP99: report.frameP99,
      errors,
    }),
  );
} finally {
  await browser.close();
}
