import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";
const out = "artifacts/performance/ammunition";
mkdirSync(out, { recursive: true });
const browser = await chromium.launch({ channel: "chrome", headless: false,
  args: ["--disable-backgrounding-occluded-windows", "--disable-renderer-backgrounding"] });
const errors = [], runs = [];
const context = await browser.newContext({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 1 });
context.on("page", page => page.on("pageerror", e => errors.push(e.message)));
try {
  const page = await context.newPage();
  // Freeze only animation scheduling so genuine wheel/pointer events can be tested
  // between fixed simulation ticks, including fast displays and catch-up frames.
  await page.addInitScript(() => {
    let frame, now;
    window.requestAnimationFrame = cb => { frame = cb; return 1; };
    window.advanceFrame = ms => { now = (now ?? performance.now()) + ms; frame(now); };
  });
  await page.goto(url); await page.waitForFunction(() => !!window.sloppy);
  await page.evaluate(() => {
    const d = window.sloppy; d.start(); window.advanceFrame(100);
    for (const t of d.sim.tanks) if (!t.human) d.sim.world.removeRigidBody(t.body);
    d.sim.tanks = [d.sim.human]; d.sim.pickups = [];
    const t = d.sim.human;
    t.ammo = { spread: 18, rocket: 12, ricochet: 24, piercing: 24 };
    t.body.setTranslation({ x: 0, y: 0.65, z: 40 }, true);
    d.sim.world.step();
  });
  await page.mouse.move(800, 450);
  const advance = () => page.evaluate(() => { for (let i = 0; i < 4; i++) window.advanceFrame(17); });
  const selected = () => page.evaluate(() => window.sloppy.sim.human.selectedAmmo);
  const zoom = await page.evaluate(() => window.sloppy.view.zoom);
  await page.mouse.wheel(0, 80);
  await page.waitForFunction(() => window.sloppy.controls.ammoSelection === 1);
  assert.equal(await selected(), "standard", "wheel waits for the next simulation tick");
  await advance(); assert.equal(await selected(), "spread");
  assert.equal(await page.evaluate(() => window.sloppy.view.zoom), zoom);
  // A burst of trackpad events cannot queue an overshoot.
  await page.evaluate(() => { window.sloppy.controls.lastAmmoScroll = performance.now(); });
  await page.mouse.wheel(0, 80); await page.mouse.wheel(0, 80); await advance();
  assert.equal(await selected(), "spread");
  await page.waitForTimeout(140); await page.mouse.wheel(0, -80);
  await page.waitForFunction(() => window.sloppy.controls.ammoSelection === -1);
  await advance(); assert.equal(await selected(), "standard");
  await page.waitForTimeout(140); await page.mouse.wheel(0, -80);
  await page.waitForFunction(() => window.sloppy.controls.ammoSelection === -1);
  await advance(); assert.equal(await selected(), "piercing");
  await page.keyboard.down("Shift"); await page.mouse.wheel(0, 80); await page.keyboard.up("Shift");
  await page.waitForFunction(n => window.sloppy.view.zoom !== n, zoom);
  assert.equal(await selected(), "piercing");
  for (const [at, delta, expected] of [[51, 80, 52], [24, -80, 23]]) {
    await page.evaluate(n => { window.sloppy.view.zoom = n; }, at);
    await page.keyboard.down("Shift"); await page.mouse.wheel(0, delta); await page.keyboard.up("Shift");
    await page.waitForFunction(n => window.sloppy.view.zoom === n, expected);
  }
  // Held fire selects before firing and drains the final special round exactly once.
  await page.evaluate(() => {
    const t = window.sloppy.sim.human;
    t.selectedAmmo = "standard"; t.ammo.spread = 1; t.cooldown = 0;
    window.sloppy.sim.shots = []; window.sloppy.sim.shotsFired = 0;
    window.sloppy.controls.lastAmmoScroll = -Infinity;
  });
  await page.mouse.down(); await page.mouse.wheel(0, 80);
  await page.waitForFunction(() => window.sloppy.controls.ammoSelection === 1);
  await advance();
  assert.equal(await selected(), "standard");
  assert.equal(await page.evaluate(() => window.sloppy.sim.shotsFired), 3);
  await page.evaluate(() => { for (let i = 0; i < 60; i++) window.advanceFrame(17); });
  await page.mouse.up();
  assert.equal(await page.evaluate(() => window.sloppy.sim.shotsFired), 4);
  await page.mouse.click(800, 450, { button: "right" }); await advance();
  assert.equal(await page.evaluate(() => window.sloppy.sim.mines.length), 1);
  // Pause button clears pending selection immediately, before an animation frame.
  await page.waitForTimeout(140); await page.mouse.wheel(0, 80);
  await page.waitForFunction(() => window.sloppy.controls.ammoSelection === 1);
  await page.locator("#pause").click();
  assert.equal(await page.evaluate(() => window.sloppy.controls.ammoSelection), undefined);
  await advance(); await page.mouse.move(800, 450); await page.mouse.wheel(0, 80);
  assert.equal(await page.evaluate(() => window.sloppy.controls.ammoSelection), undefined);
  await page.locator("#resume").click(); await advance();
  await page.evaluate(() => {
    const d = window.sloppy, t = d.sim.human;
    t.protection = 0; d.sim.damageTank(t, 999, t.id, t.team);
  });
  await page.mouse.wheel(0, 80);
  assert.equal(await page.evaluate(() => window.sloppy.controls.ammoSelection), undefined);
  assert.equal(await selected(), "standard");
  await page.evaluate(() => { window.sloppy.restart(); });
  assert.equal(await page.evaluate(() => window.sloppy.controls.ammoSelection), undefined);
  console.log("Real wheel, Shift-wheel limits, throttling, held fire, depletion, mines, pause, death and reset passed.");

  // A controlled presentation fixture uses the actual production pickup meshes and HUD.
  await page.evaluate(() => {
    const d = window.sloppy; d.start(); d.view.zoom = 23;
    const t = d.sim.human;
    t.body.setTranslation({ x: 0, y: 0.65, z: 40 }, true);
    t.previous = { x: 0, z: 40 }; t.selectedAmmo = "rocket";
    t.ammo = { spread: 18, rocket: 12, ricochet: 0, piercing: 24 };
    for (const [i, kind] of ["spread", "rocket", "ricochet", "piercing"].entries()) {
      const p = d.sim.pickups.find(p => p.kind === kind);
      p.x = (i - 1.5) * 4; p.z = 36;
    }
    d.view.reset(d.sim); d.sim.world.step();
    for (let i = 0; i < 10; i++) window.advanceFrame(17);
  });
  await page.waitForTimeout(400); await advance();
  assert.equal(await page.locator(".ammo-slot").count(), 5);
  assert.equal(await page.locator("#ammo-rocket").getAttribute("aria-label"), "ROCKET: 12, selected");
  assert.match(await page.locator("#ammo-ricochet").getAttribute("class"), /empty/);
  await page.screenshot({ path: `${out}/hud-crates.png` });
  await page.setViewportSize({ width: 600, height: 780 }); await advance();
  const bounds = await page.locator(".combat-status").boundingBox();
  assert.ok(bounds.x >= 0 && bounds.x + bounds.width <= 600);
  await page.screenshot({ path: `${out}/hud-narrow.png` });
  await page.close();
  if (process.argv.includes("--visual-only")) {
    assert.deepEqual(errors, []);
    console.log("HUD/crate screenshots and narrow-layout checks passed; no page errors.");
    await browser.close();
    process.exit(0);
  }

  const perf = await context.newPage();
  await perf.setViewportSize({ width: 1600, height: 900 });
  for (const scenario of ["standard", "plentiful-spread-piercing"]) for (const seed of [12345, 45678, 98765]) {
    await perf.goto(`${url}?autoplay`); await perf.waitForFunction(() => !!window.sloppy);
    await perf.evaluate(({ scenario, seed }) => {
      const d = window.sloppy; d.sim.seed = seed; d.sim.roundCount = 24;
      d.sim.humanTeam = 0; d.start(); d.autoplay(); d.overview();
      const sim = d.sim, step = sim.step.bind(sim);
      sim.step = (...args) => {
        for (const [i, t] of sim.tanks.entries()) {
          // Deliberately abundant supply, continuous combat and matching chassis/roles.
          t.ammo = { spread: scenario === "standard" ? 0 : 36,
            piercing: scenario === "standard" ? 0 : 48, rocket: 0, ricochet: 0 };
          if (scenario !== "standard") t.ammo[i % 2 ? "spread" : "piercing"] = 0;
          t.rapid = 12;
        }
        step(...args);
      };
      d.record();
    }, { scenario, seed });
    await perf.waitForTimeout(15000);
    const report = await perf.evaluate(() => window.sloppy.stop());
    assert.ok(report.snapshot.elapsed > 13, "simulation must keep up");
    runs.push({ scenario, seed, ...report });
    console.log(JSON.stringify({ scenario, seed, fps: report.fps, p99: report.frameP99,
      simulationMean: report.simulationMean, simulationP95: report.simulationP95,
      renderMean: report.renderMean, maxProjectiles: report.maxProjectiles }));
  }
  assert.deepEqual(errors, []);
  writeFileSync("artifacts/ammunition-results.json", JSON.stringify({
    date: new Date().toISOString(), browser: browser.version(), resolution: [1600, 900],
    workload: "24 tanks, rapid fire, 5 seconds warmup + 10 seconds measured; standard vs continually refilled spread/piercing; three seeds",
    input: "real wheel, Shift-wheel limits, throttle, held fire, depletion, mines, pause, death, reset and narrow HUD passed",
    errors, runs,
  }, null, 2));
} finally { await browser.close(); }
