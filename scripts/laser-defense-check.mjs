import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5179/sloppy-tanks/";
const out = "artifacts/performance/laser-defense";
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
  await page.evaluate(() => {
    const d = window.sloppy;
    d.start();
    d.autoplay(false);
    const s = d.sim,
      t = s.human;
    for (const bot of s.tanks) if (bot !== t) s.world.removeRigidBody(bot.body);
    s.tanks = [t];
    s.shots = [];
    s.events = [];
    s.pickups = s.pickups.filter((p) => p.kind === "laser");
    t.body.setTranslation({ x: 0, y: 0.65, z: 5 }, true);
    t.previous = { x: 0, z: 5 };
    t.protection = 0;
    d.view.reset(s);
    d.view.zoom = 23;
    s.world.step();
    window.advanceFrame(17);
  });
  const advance = (n = 4) =>
    page.evaluate((n) => {
      for (let i = 0; i < n; i++) window.advanceFrame(17);
    }, n);
  assert.equal(await page.evaluate(() => window.sloppy.sim.pickups[0].available), false);
  assert.deepEqual(
    await page.evaluate(() => {
      const d = window.sloppy,
        p = d.sim.pickups[0],
        g = d.view.pickupMeshes.get(p.id);
      return {
        podium: g.visible,
        gem: g.userData.gem.visible,
        refill: g.userData.refill.visible,
        progress: g.userData.refill.geometry.drawRange.count,
        duration: p.cooldownDuration,
      };
    }),
    { podium: true, gem: false, refill: true, progress: 0, duration: 25 },
  );
  await advance(1480);
  assert.equal(await page.evaluate(() => window.sloppy.sim.pickups[0].available), true);
  await page.screenshot({ path: `${out}/rare-pickup.png` });
  // Drive through the pickup using the real control path.
  await page.keyboard.down("w");
  // Collection depends on the chassis acceleration, not a fixed number of display frames.
  let collected = false;
  for (let frame = 0; frame < 120; frame++) {
    await advance(1);
    collected = await page.evaluate(() => window.sloppy.sim.human.laser > 0);
    if (collected) break;
  }
  assert.ok(collected, "W-key driving must reach the visible laser pickup within two seconds");
  await page.keyboard.up("w");
  await advance(5);
  assert.ok(await page.evaluate(() => window.sloppy.sim.human.laser > 5));
  assert.match(await page.locator("#toast").innerText(), /LASER DEFENSE/);
  assert.match(await page.locator("#effects").innerText(), /LASER DEFENSE/);
  assert.equal(await page.evaluate(() => window.sloppy.view.laserVisuals.lens.count), 1);
  assert.equal(await page.evaluate(() => window.sloppy.sim.pickups[0].available), false);
  assert.deepEqual(
    await page.evaluate(() => {
      const d = window.sloppy,
        p = d.sim.pickups[0],
        g = d.view.pickupMeshes.get(p.id);
      return {
        podium: g.visible,
        gem: g.userData.gem.visible,
        refill: g.userData.refill.visible,
        duration: p.cooldownDuration,
      };
    }),
    { podium: true, gem: false, refill: true, duration: 45 },
  );
  checks.push(
    "Rare pickup starts on an empty 25-second podium, appears on time, and leaves a 45-second podium after real W-key collection",
  );
  // Pin the fixture for a visible successful rocket interception; the real game loop handles it.
  await page.mouse.click(800, 350);
  await page.evaluate(() => window.sloppy.audio.start());
  await page.waitForFunction(() => window.sloppy.audio.sounds.laser.state() === "loaded");
  await page.evaluate(() => {
    const d = window.sloppy,
      s = d.sim,
      t = s.human;
    s.shots = [];
    s.events = [];
    t.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
    t.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    t.previous = { x: 0, z: 0 };
    t.laser = 6;
    s.world.step();
    d.view.reset(s);
    window.laserCalls = 0;
    const sound = d.audio.sounds.laser,
      play = sound.play.bind(sound);
    sound.play = (...args) => {
      if (!args.length) window.laserCalls++;
      return play(...args);
    };
    window.originalRandom = s.rng.next.bind(s.rng);
    s.rng.next = () => 0;
    s.shots.push({
      id: s.nextId++,
      x: -4,
      z: -4,
      vx: 12,
      vz: 12,
      team: 1 - t.team,
      owner: -99,
      weapon: "rocket",
      damage: 65,
      bounces: 0,
      piercing: 0,
      life: 3.5,
    });
    window.advanceFrame(17);
  });
  assert.equal(await page.evaluate(() => window.sloppy.sim.shots.length), 0);
  assert.equal(await page.evaluate(() => window.sloppy.view.laserVisuals.core.count), 1);
  assert.equal(await page.evaluate(() => window.laserCalls), 1);
  await page.screenshot({ path: `${out}/rocket-intercept.png` });
  await advance(9);
  assert.equal(await page.evaluate(() => window.sloppy.view.laserVisuals.core.count), 0);
  assert.equal(await page.evaluate(() => window.sloppy.sim.human.hp), 100);
  const beforePause = await page.evaluate(() => window.sloppy.sim.human.laser);
  await page.keyboard.press("Escape");
  await advance(60);
  assert.equal(await page.evaluate(() => window.sloppy.sim.human.laser), beforePause);
  await page.locator("#resume").click();
  await advance(365);
  assert.equal(await page.evaluate(() => window.sloppy.sim.human.laser), 0);
  assert.equal(await page.evaluate(() => window.sloppy.view.laserVisuals.lens.count), 0);
  assert.doesNotMatch(await page.locator("#effects").innerText(), /LASER/);
  checks.push(
    "Rocket vaporizes with a thin beam and saved zap, no damage; beam expires, pause freezes timer, six-second expiry clears lens and HUD",
  );
  const resources = await page.evaluate(async () => {
    const d = window.sloppy;
    const sound = d.audio.sounds.laser;
    await new Promise((resolve, reject) => {
      const id = sound.play();
      sound.once("end", resolve, id);
      sound.once("playerror", reject, id);
    });
    const geometry = [];
    for (let i = 0; i < 10; i++) {
      d.restart();
      window.advanceFrame(17);
      geometry.push(d.view.renderer.info.memory.geometries);
    }
    return { duration: sound.duration(), geometry };
  });
  assert.equal(new Set(resources.geometry).size, 1);
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
    window.intercepts = 0;
    const event = d.view.event.bind(d.view);
    d.view.event = (...args) => {
      if (args[0].type === "laser") window.intercepts++;
      event(...args);
    };
    const step = d.sim.step.bind(d.sim);
    d.sim.step = (...args) => {
      // Deliberately stronger than the rare pickup's normal single-tank workload.
      for (const t of d.sim.tanks) {
        t.laser = 6;
        t.rapid = 12;
        t.ammo = { spread: 36, rocket: 24, ricochet: 48, piercing: 48 };
      }
      step(...args);
    };
    d.record();
  });
  await live.waitForTimeout(20000);
  const report = await live.evaluate(() => ({
    ...window.sloppy.stop(),
    interceptions: window.intercepts,
  }));
  assert.ok(report.interceptions > 0);
  assert.ok(report.snapshot.elapsed > 18);
  assert.deepEqual(errors, []);
  writeFileSync(
    "artifacts/laser-defense-results.json",
    JSON.stringify(
      {
        date: new Date().toISOString(),
        browser: browser.version(),
        checks,
        resources,
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
        checks,
        resources,
        fps: report.fps,
        p99: report.frameP99,
        simulationMean: report.simulationMean,
        renderMean: report.renderMean,
        interceptions: report.interceptions,
        errors,
      },
      null,
      2,
    ),
  );
} finally {
  await browser.close();
}
