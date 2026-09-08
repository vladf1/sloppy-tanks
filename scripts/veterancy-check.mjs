import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5179/sloppy-tanks/";
const out = "artifacts/performance/veterancy";
mkdirSync(out, { recursive: true });
const browser = await chromium.launch({ channel: "chrome", headless: false,
  args: ["--disable-backgrounding-occluded-windows", "--disable-renderer-backgrounding"] });
const errors = [], checks = [];
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 1 });
  context.on("page", p => p.on("pageerror", e => errors.push(e.message)));
  const page = await context.newPage();
  await page.addInitScript(() => {
    let frame, now;
    window.requestAnimationFrame = cb => { frame = cb; return 1; };
    window.advanceFrame = ms => { now = (now ?? performance.now()) + ms; frame(now); };
  });
  await page.goto(url); await page.waitForFunction(() => !!window.sloppy);
  await page.evaluate(() => {
    const d = window.sloppy; d.start(); d.autoplay(false); const s = d.sim, t = s.human;
    const enemy = s.tanks.find(b => b.team !== t.team);
    for (const b of s.tanks) if (b !== t && b !== enemy) s.world.removeRigidBody(b.body);
    s.tanks = [t, enemy]; s.pickups = []; s.events = [];
    for (const c of s.covers) s.world.removeRigidBody(c.body);
    s.covers = []; s.coverByCollider.clear(); s.nav.rebuild([]);
    for (const [b, z] of [[t, 0], [enemy, -7]]) {
      b.body.setTranslation({ x: 0, y: 0.65, z }, true); b.previous = { x: 0, z };
      b.protection = 0; b.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
    }
    // One real hit must cross the threshold; leave the bot unable to fire/move during this fixture.
    t.xp = 180; enemy.brain.reaction = 999; enemy.brain.decision = 999;
    enemy.body.setEnabledTranslations(false, true, false, true);
    d.view.reset(s); d.view.zoom = 23; s.world.step();
    window.advanceFrame(17);
  });
  const advance = (n = 4) => page.evaluate(n => { for (let i = 0; i < n; i++) window.advanceFrame(17); }, n);
  await page.mouse.click(800, 420); // Unlock audio in a real user gesture.
  await page.waitForFunction(() => window.sloppy.audio.sounds.promotion.state() === "loaded");
  const target = await page.evaluate(async () => {
    const d = window.sloppy, s = d.sim, t = s.human;
    window.promotionsHeard = 0;
    const sound = d.audio.sounds.promotion, play = sound.play.bind(sound);
    sound.play = (...args) => { if (!args.length) window.promotionsHeard++; return play(...args); };
    const point = d.view.camera.position.clone().set(0, 0, -7).project(d.view.camera);
    return { x: (point.x + 1) * innerWidth / 2, y: (1 - point.y) * innerHeight / 2 };
  });
  await page.mouse.move(target.x, target.y); await page.mouse.down(); await advance(24); await page.mouse.up();
  assert.equal(await page.locator("#rank").innerText(), "VETERAN");
  assert.equal(await page.locator("#xp, #xpbar, .veterancy").count(), 0);
  assert.match(await page.locator("#toast").innerText(), /PROMOTED TO VETERAN/);
  assert.equal(await page.evaluate(() => window.promotionsHeard), 1);
  assert.equal(await page.evaluate(() => window.sloppy.view.bars.get(window.sloppy.sim.human.id).userData.ranks.filter(c => c.visible).length), 1);
  await page.waitForTimeout(180); await page.screenshot({ path: `${out}/player-promotion.png` });
  checks.push("Real mouse aim and firing earn hull-damage XP, promote to Veteran, show rank beside tank name/chevron/toast and play the saved chime");
  await page.evaluate(async () => {
    const d = window.sloppy, s = d.sim, t = s.human, bot = s.tanks[1];
    t.xp = 499; s.damageTank(bot, 1, t.id, t.team);
    // Bot promotes through real damage credit too, with a small nudge to reach the threshold.
    bot.xp = 990; s.damageTank(t, 10, bot.id, bot.team);
    window.advanceFrame(17);
  });
  await advance();
  assert.equal(await page.locator("#rank").innerText(), "ELITE");
  assert.equal(await page.evaluate(() => window.sloppy.view.bars.get(window.sloppy.sim.tanks[1].id).userData.ranks.filter(c => c.visible).length), 3);
  await page.waitForTimeout(180); await page.screenshot({ path: `${out}/elite-and-heroic-bot.png` });
  // Freeze input while confirming the full self-repair delay and pause behavior.
  const before = await page.evaluate(() => window.sloppy.sim.human.hp);
  await page.keyboard.press("Escape"); await advance(360);
  assert.equal(await page.evaluate(() => window.sloppy.sim.human.hp), before);
  await page.locator("#resume").click(); await advance(280);
  assert.equal(await page.evaluate(() => window.sloppy.sim.human.hp), before);
  await advance(80);
  assert.ok(await page.evaluate(() => window.sloppy.sim.human.hp) > before);
  assert.match(await page.locator("#effects").innerText(), /SELF-REPAIR/);
  checks.push("Bot earns Heroic rank through damage with three small chevrons; Elite repair waits five combat-free seconds and freezes while paused");
  await page.evaluate(async () => {
    const s = window.sloppy.sim; s.human.xp = 999;
    s.damageTank(s.tanks[1], 1, s.human.id, s.human.team); window.advanceFrame(17);
  });
  await advance();
  assert.equal(await page.locator(".tank-label #rank").innerText(), "HEROIC");
  await page.setViewportSize({ width: 700, height: 700 }); await advance();
  await page.waitForTimeout(180); await page.screenshot({ path: `${out}/compact-hud.png` });
  assert.ok(await page.locator(".combat-status").evaluate(e => e.scrollWidth <= e.clientWidth));
  assert.ok(await page.locator(".tank-label").evaluate(e => e.getBoundingClientRect().right <= innerWidth));
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.evaluate(() => {
    const s = window.sloppy.sim; s.damageTank(s.human, 9999, s.human.id, s.human.team);
  });
  await advance(185);
  assert.equal(await page.locator("#rank").innerText(), "ROOKIE");
  assert.equal(await page.locator("#xp, #xpbar").count(), 0);
  assert.equal(await page.evaluate(() => window.sloppy.view.bars.get(window.sloppy.sim.human.id).userData.ranks.filter(c => c.visible).length), 0);
  checks.push("Heroic rank sits beside the tank name, narrow HUD fits, death/respawn clears experience, bonuses and rank chevrons");
  const resources = await page.evaluate(async () => {
    const d = window.sloppy, sound = d.audio.sounds.promotion;
    await new Promise((resolve, reject) => { const id = sound.play(); sound.once("end", resolve, id); sound.once("playerror", reject, id); });
    const geometry = [];
    for (let i = 0; i < 10; i++) { d.restart(); window.advanceFrame(17); geometry.push(d.view.renderer.info.memory.geometries); }
    return { duration: sound.duration(), geometry };
  });
  assert.equal(new Set(resources.geometry).size, 1);
  await page.close();
  const live = await context.newPage();
  await live.goto(`${url}?autoplay`); await live.waitForFunction(() => !!window.sloppy);
  await live.evaluate(() => {
    const d = window.sloppy; d.sim.seed = 12345; d.sim.roundCount = 24; d.sim.mapMode = "random";
    d.start(); d.autoplay(); window.naturalPromotions = 0;
    const event = d.view.event.bind(d.view); d.view.event = (...args) => {
      if (args[0].type === "promotion") window.naturalPromotions++; event(...args);
    };
    d.record();
  });
  await live.waitForTimeout(25000);
  const report = await live.evaluate(() => ({ ...window.sloppy.stop(), promotions: window.naturalPromotions }));
  assert.ok(report.promotions > 0); assert.ok(report.snapshot.elapsed > 23); assert.deepEqual(errors, []);
  writeFileSync("artifacts/veterancy-results.json", JSON.stringify({
    date: new Date().toISOString(), browser: browser.version(), checks, resources, report, errors,
  }, null, 2));
  console.log(JSON.stringify({ checks, resources, fps: report.fps, p99: report.frameP99,
    simulationMean: report.simulationMean, renderMean: report.renderMean, naturalPromotions: report.promotions, errors }, null, 2));
} finally { await browser.close(); }
