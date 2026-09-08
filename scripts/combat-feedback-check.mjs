import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5179/sloppy-tanks/";
const out = "artifacts/performance/combat-feedback";
mkdirSync(out, { recursive: true });
const browser = await chromium.launch({ channel: "chrome", headless: false,
  args: ["--disable-backgrounding-occluded-windows", "--disable-renderer-backgrounding"] });
const errors = [], checks = [];
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", e => errors.push(e.message));
  await page.addInitScript(() => {
    let frame, now;
    window.requestAnimationFrame = cb => { frame = cb; return 1; };
    window.advanceFrame = ms => { now = (now ?? performance.now()) + ms; frame(now); };
  });
  await page.goto(url); await page.waitForFunction(() => !!window.sloppy);
  await page.evaluate(() => {
    const d = window.sloppy; d.start(); window.advanceFrame(17);
    const s = d.sim, t = s.human;
    for (const bot of s.tanks) if (bot !== t) s.world.removeRigidBody(bot.body);
    s.tanks = [t]; s.shots = []; s.events = [];
    t.body.setTranslation({ x: 0, y: 0.65, z: 40 }, true); t.previous = { x: 0, z: 40 };
    t.protection = 0; t.ammo = { spread: 18, rocket: 12, ricochet: 24, piercing: 24 };
    s.world.step(); d.view.zoom = 23;
    window.soundCalls = [];
    for (const [key, sound] of Object.entries(d.audio.sounds)) {
      const play = sound.play.bind(sound);
      sound.play = (...args) => {
        // Howler also calls play(id, true) internally when setting spatial state.
        if (args.length === 0) window.soundCalls.push(key);
        return play(...args);
      };
    }
    window.advanceFrame(17);
  });
  await page.mouse.click(900, 450); // Real gesture unlocks Web Audio (and fires one shell).
  await page.evaluate(() => window.sloppy.audio.start());
  await page.waitForFunction(() => Object.values(window.sloppy.audio.sounds).every(s => s.state() === "loaded"));
  // The HUD refreshes every fourth render frame.
  const advance = (count = 4) => page.evaluate(n => { for (let i = 0; i < n; i++) window.advanceFrame(17); }, count);
  const selected = () => page.evaluate(() => window.sloppy.sim.human.selectedAmmo);
  for (const [key, expected] of [["e", "spread"], ["q", "standard"], ["q", "piercing"],
    ["1", "standard"], ["2", "spread"], ["3", "rocket"], ["4", "ricochet"], ["5", "piercing"]]) {
    await page.keyboard.press(key); await advance(); assert.equal(await selected(), expected);
  }
  await page.evaluate(() => { const t = window.sloppy.sim.human; t.ammo.rocket = 0; t.cooldown = 0.6; });
  await page.keyboard.press("3"); await advance(); assert.equal(await selected(), "piercing");
  await page.keyboard.press("1"); await advance();
  assert.ok(await page.evaluate(() => window.sloppy.sim.human.cooldown > 0.4));
  assert.equal(await page.evaluate(() => window.sloppy.view.reticleInk.opacity), 0.3);
  await page.screenshot({ path: `${out}/reloading.png` });
  await advance(40);
  assert.equal(await page.evaluate(() => window.sloppy.view.reticleInk.opacity), 1);
  await page.screenshot({ path: `${out}/ready.png` });
  // The production event route plays each actual shot's weapon, including the last round.
  for (const [key, weapon] of [["1", "shot"], ["2", "shot-spread"], ["3", "shot-rocket"],
    ["4", "shot-ricochet"], ["5", "shot-piercing"]]) {
    await page.evaluate(() => {
      const t = window.sloppy.sim.human; t.ammo.rocket = 2; t.cooldown = 0;
      window.soundCalls = []; window.sloppy.audio.lastShot = performance.now();
    });
    await page.keyboard.press(key); await page.mouse.down(); await advance(); await page.mouse.up();
    assert.ok((await page.evaluate(() => window.soundCalls)).includes(weapon), weapon);
  }
  checks.push("Real Q/E, 1–5, wrap, empty-slot rejection, cooldown preservation, ready/dim reticle and all five shot sounds");
  // Feedback must follow credited hull damage, not cover, another shooter or self damage.
  await page.evaluate(() => {
    const d = window.sloppy, s = d.sim, t = s.human;
    s.shots = []; s.events = []; d.view.hitConfirmUntil = 0; window.soundCalls = [];
    const target = s.addTank(1 - t.team, false, "balanced", 1);
    target.body.setTranslation({ x: 0, y: 0.65, z: 33 }, true);
    target.previous = { x: 0, z: 33 }; target.protection = 0;
    target.brain.decision = 100; target.brain.goal = { x: 0, z: 33 };
    d.view.reset(s); s.world.step(); window.targetId = target.id;
    s.damageTank(target, 1, -123, t.team);
    s.damageTank(t, 1, t.id, t.team);
    s.events.push({ type: "impact", x: 0, z: 34 });
    window.advanceFrame(17);
  });
  assert.equal(await page.evaluate(() => window.sloppy.view.crosshair.scale.x), 1);
  assert.ok(!(await page.evaluate(() => window.soundCalls)).includes("hit"));
  await page.evaluate(() => {
    const d = window.sloppy, s = d.sim, t = s.human, target = s.tanks.find(t => t.id === window.targetId);
    d.audio.lastHit = -Infinity; window.soundCalls = []; t.cooldown = 0.5;
    s.damageTank(target, 10, t.id, t.team); s.damageTank(target, 10, t.id, t.team);
    window.advanceFrame(17);
  });
  assert.equal(await page.evaluate(() => window.sloppy.view.crosshair.scale.x), 1.2);
  assert.equal(await page.evaluate(() => window.sloppy.view.reticleInk.opacity), 1);
  assert.equal((await page.evaluate(() => window.soundCalls)).filter(k => k === "hit").length, 1);
  await page.screenshot({ path: `${out}/confirmed-hit.png` });
  await advance(12);
  assert.equal(await page.evaluate(() => window.sloppy.view.crosshair.scale.x), 1);
  assert.equal(await page.evaluate(() => window.sloppy.view.reticleInk.opacity), 0.3);
  await page.evaluate(() => {
    const d = window.sloppy, s = d.sim, t = s.human, target = s.tanks.find(t => t.id === window.targetId);
    d.audio.lastHit = -Infinity; window.soundCalls = [];
    s.damageTank(target, 999, t.id, t.team); window.advanceFrame(17);
    s.tanks = [t]; // Lethal damage already removed the target's physics body.
  });
  assert.equal(await page.evaluate(() => window.sloppy.view.crosshair.scale.x), 1.2);
  assert.ok((await page.evaluate(() => window.soundCalls)).includes("hit"));
  checks.push("Surviving and lethal credited hits flash/tick; self, other shooter and cover do not; pellet ticks coalesce; flash expires");
  // Pause owns both shortcuts and the existing status panel's animation.
  await page.evaluate(() => { window.sloppy.sim.human.hp = 25; }); await advance();
  assert.ok(!await page.locator(".status").evaluate(e => e.classList.contains("critical-health")));
  await page.evaluate(() => { window.sloppy.sim.human.hp = 24; }); await advance();
  assert.ok(await page.locator(".status").evaluate(e => e.classList.contains("critical-health")));
  await page.keyboard.press("Escape"); await advance();
  assert.match(await page.locator(".menu").innerText(), /Q \/ E or scroll/);
  assert.match(await page.locator(".menu").innerText(), /Shift \+ scroll zooms/);
  const pausedSelection = await selected();
  await page.keyboard.press("1"); await advance(); assert.equal(await selected(), pausedSelection);
  assert.equal(await page.locator(".status").evaluate(e => getComputedStyle(e).animationPlayState), "paused");
  await page.locator("#resume").click(); await advance();
  assert.equal(await page.locator(".status").evaluate(e => getComputedStyle(e).animationPlayState), "running");
  await page.evaluate(() => { window.sloppy.sim.human.hp = 100; }); await advance();
  assert.ok(!await page.locator(".status").evaluate(e => e.classList.contains("critical-health")));
  checks.push("HP threshold, repair clearing, paused animation, paused shortcuts and shared menu hints");
  // Arrange the real production crates at three refill stages and one stocked pad.
  await page.evaluate(() => {
    const d = window.sloppy, s = d.sim; s.shots = []; s.events = []; s.human.hp = 24;
    s.pickups = ["spread", "rocket", "ricochet", "piercing"].map((kind, i) => ({
      id: s.nextId++, kind, x: (i - 1.5) * 4, z: 36,
      available: i === 3, cooldown: [13, 6.5, 1, 0][i],
    }));
    d.view.reset(s); window.advanceFrame(17);
  });
  await advance();
  const pads = () => page.evaluate(() => window.sloppy.sim.pickups.map(p => {
    const g = window.sloppy.view.pickupMeshes.get(p.id);
    return { available: p.available, visible: g.visible, gem: g.userData.gem.visible,
      progress: g.userData.refill.geometry.drawRange.count, ring: g.userData.ring.material.opacity };
  }));
  const initialPads = await pads();
  assert.ok(initialPads.every(p => p.visible));
  assert.deepEqual(initialPads.map(p => p.gem), [false, false, false, true]);
  assert.ok(initialPads[0].progress < initialPads[1].progress && initialPads[1].progress < initialPads[2].progress);
  assert.deepEqual(initialPads.map(p => p.ring), [0.2, 0.2, 0.2, 1]);
  await page.screenshot({ path: `${out}/refill-pads-low-health.png` });
  await page.keyboard.press("Escape"); await advance(); const pausedPads = await pads();
  await advance(60); assert.deepEqual(await pads(), pausedPads);
  await page.locator("#resume").click(); await advance(780);
  assert.ok((await pads()).every(p => p.available && p.gem && p.ring === 1));
  await page.setViewportSize({ width: 600, height: 780 });
  await page.waitForFunction(() => window.sloppy.view.renderer.domElement.width === 600);
  await advance();
  const bounds = await page.locator(".combat-status").boundingBox();
  assert.ok(bounds.x >= 0 && bounds.x + bounds.width <= 600);
  await page.screenshot({ path: `${out}/narrow-low-health.png` });
  await page.evaluate(() => {
    const s = window.sloppy.sim; s.damageTank(s.human, 999, s.human.id, s.human.team); window.advanceFrame(17);
  });
  await advance();
  assert.ok(!await page.locator(".status").evaluate(e => e.classList.contains("critical-health")));
  await page.keyboard.press("5");
  assert.equal(await page.evaluate(() => window.sloppy.controls.ammoSelection), undefined);
  checks.push("Dim ammo pads show refill progress, pause freezes it, all crates reappear after 13 seconds; narrow HUD and death clearing");
  // Check actual decoded playback of every saved file, with completion callbacks.
  const durations = await page.evaluate(async () => {
    const sounds = window.sloppy.audio.sounds;
    const result = {};
    for (const [key, sound] of Object.entries(sounds)) {
      await new Promise((resolve, reject) => {
        const id = sound.play(); sound.once("end", resolve, id); sound.once("playerror", reject, id);
      });
      result[key] = sound.duration();
    }
    return result;
  });
  assert.equal(Object.keys(durations).length, 11);
  const geometries = await page.evaluate(() => {
    const d = window.sloppy, counts = [];
    for (let i = 0; i < 10; i++) { d.restart(); window.advanceFrame(17); counts.push(d.view.renderer.info.memory.geometries); }
    return counts;
  });
  assert.equal(new Set(geometries).size, 1);
  checks.push("All ten saved MP3s decode/play to completion; ten rendered resets have stable geometry counts");
  await page.close();
  const perf = await context.newPage(); perf.on("pageerror", e => errors.push(e.message));
  await perf.goto(`${url}?autoplay`); await perf.waitForFunction(() => !!window.sloppy);
  await perf.evaluate(() => {
    const d = window.sloppy; d.sim.seed = 12345; d.sim.roundCount = 24; d.sim.mapMode = "random";
    d.start(); d.autoplay(); d.record();
  });
  await perf.waitForTimeout(20000);
  const report = await perf.evaluate(() => window.sloppy.stop());
  assert.ok(report.snapshot.elapsed > 18); assert.deepEqual(errors, []);
  writeFileSync("artifacts/combat-feedback-results.json", JSON.stringify({
    date: new Date().toISOString(), browser: browser.version(), checks, initialPads, durations, geometries, report, errors,
  }, null, 2));
  console.log(JSON.stringify({ checks, fps: report.fps, frameP99: report.frameP99, errors }, null, 2));
} finally { await browser.close(); }
