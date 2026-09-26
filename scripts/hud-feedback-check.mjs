// HUD, reticle, rank, laser and pickup feedback with real mouse, wheel and keyboard
// input, the real game loop at fixed frame steps, and the saved sounds. Rules behind
// this feedback (selection, XP, laser timing, refills) are covered by tests/*.test.ts.
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { freezeLoop, gameUrl, launchGame, startRound } from "./browser-helpers.mjs";

const out = "artifacts/performance/hud-feedback";
mkdirSync(out, { recursive: true });
const { browser, page, errors } = await launchGame();
const checks = [];
try {
  await freezeLoop(page);
  await page.goto(gameUrl);
  await startRound(page);
  // One cleared arena: the player at the origin facing a frozen enemy 7 m north.
  await page.evaluate(() => {
    const d = window.sloppy;
    d.autoplay(false);
    const s = d.sim,
      t = s.human;
    const enemy = s.tanks.find((b) => b.team !== t.team);
    for (const b of s.tanks) if (b !== t && b !== enemy) s.world.removeRigidBody(b.body);
    s.tanks = [t, enemy];
    window.laserPickup = s.pickups.find((p) => p.kind === "laser");
    s.pickups = [];
    s.shots = [];
    s.events = [];
    for (const c of s.covers) s.world.removeRigidBody(c.body);
    s.covers = [];
    s.movableCovers = [];
    s.coverByCollider.clear();
    s.nav.rebuild([]);
    for (const [b, z] of [
      [t, 0],
      [enemy, -7],
    ]) {
      b.body.setTranslation({ x: 0, y: 0.65, z }, true);
      b.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
      b.previous = { x: 0, z };
      b.protection = 0;
    }
    t.ammo = { spread: 18, rocket: 12, ricochet: 0, piercing: 24 };
    enemy.brain.reaction = 999;
    enemy.brain.decision = 999;
    enemy.body.setEnabledTranslations(false, true, false, true);
    window.enemyId = enemy.id;
    d.view.reset(s);
    d.view.zoom = 23;
    s.world.step();
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
  // The HUD refreshes every fourth rendered frame.
  const advance = (count = 4) =>
    page.evaluate((n) => {
      for (let i = 0; i < n; i++) window.advanceFrame(17);
    }, count);
  const human = (key) => page.evaluate((key) => window.sloppy.sim.human[key], key);
  const view = (path) =>
    page.evaluate((path) => path.split(".").reduce((o, k) => o[k], window.sloppy.view), path);
  const sounds = () => page.evaluate(() => window.soundCalls.splice(0));
  const panelHeight = () => page.locator(".combat-status").evaluate((e) => e.offsetHeight);
  const chevrons = (tankId) =>
    page.evaluate(
      (id) =>
        window.sloppy.view.bars
          .get(id ?? window.sloppy.sim.human.id)
          .userData.ranks.filter((c) => c.visible).length,
      tankId,
    );
  const enemyId = await page.evaluate(() => window.enemyId);
  const south = { x: 800, y: 800 }; // Aims away from the enemy into the empty arena.
  // A real gesture unlocks Web Audio (and fires one harmless shell).
  await page.mouse.click(south.x, south.y);
  await page.evaluate(() => window.sloppy.audio.start());
  await page.waitForFunction(() =>
    Object.values(window.sloppy.audio.sounds).every((s) => s.state() === "loaded"),
  );
  await advance();

  // Hovering and focusing ammo slots must not resize the status panel.
  const emptyPanel = await panelHeight();
  for (const slot of await page.locator(".ammo-slot").all()) {
    await slot.hover();
    await slot.focus();
    await advance();
    assert.equal(await panelHeight(), emptyPanel, "hover/focus keeps the panel height");
  }
  await page.mouse.move(south.x, south.y);
  // One selection by wheel, applied on the next simulation tick, and one by key.
  await page.mouse.wheel(0, 80);
  await page.waitForFunction(() => window.sloppy.controls.ammoSelection === 1);
  assert.equal(await human("selectedAmmo"), "standard", "wheel waits for the next tick");
  await advance();
  assert.equal(await human("selectedAmmo"), "spread");
  await page.keyboard.press("3");
  await advance();
  assert.equal(await human("selectedAmmo"), "rocket");
  assert.equal(await page.locator(".ammo-slot").count(), 5);
  assert.equal(
    await page.locator("#ammo-rocket").getAttribute("aria-label"),
    "3: ROCKET, 12 remaining, selected",
  );
  assert.match(await page.locator("#ammo-ricochet").getAttribute("class"), /empty/);
  await page.screenshot({ path: `${out}/ammo-hud.png` });
  // Reloading dims the reticle until the cannon is ready.
  await page.evaluate(() => {
    window.sloppy.sim.human.cooldown = 0.6;
  });
  await advance();
  assert.equal(await view("reticleInk.opacity"), 0.3);
  await advance(40);
  assert.equal(await view("reticleInk.opacity"), 1);
  // The production event route plays each actual shot's weapon sound.
  for (const [weapon, sound] of [
    ["standard", "shot"],
    ["spread", "shot-spread"],
    ["rocket", "shot-rocket"],
    ["ricochet", "shot-ricochet"],
    ["piercing", "shot-piercing"],
  ]) {
    await page.evaluate((weapon) => {
      const t = window.sloppy.sim.human;
      t.ammo = { spread: 2, rocket: 2, ricochet: 2, piercing: 2 };
      t.selectedAmmo = weapon;
      t.cooldown = 0;
      window.soundCalls = [];
      window.sloppy.audio.lastShot = performance.now();
    }, weapon);
    await page.mouse.down();
    await advance();
    await page.mouse.up();
    assert.ok((await sounds()).includes(sound), sound);
  }
  checks.push(
    "Hover-stable ammo panel, wheel and key selection, slot labels, reload reticle, shot sounds",
  );

  // Real mouse aim and fire earn the XP that promotes the player.
  await page.evaluate(() => {
    const s = window.sloppy.sim;
    s.shots = [];
    s.human.selectedAmmo = "standard";
    s.human.cooldown = 0;
    s.human.xp = 280;
    window.soundCalls = [];
  });
  const target = await page.evaluate(() => {
    const { camera } = window.sloppy.view;
    const point = camera.position.clone().set(0, 0, -7).project(camera);
    return { x: ((point.x + 1) * innerWidth) / 2, y: ((1 - point.y) * innerHeight) / 2 };
  });
  await page.mouse.move(target.x, target.y);
  await page.mouse.down();
  await advance(24);
  await page.mouse.up();
  assert.equal(await page.locator("#rank").innerText(), "VETERAN");
  assert.equal(await page.locator("#xp, #xpbar, .veterancy").count(), 0);
  assert.match(await page.locator("#toast").innerText(), /PROMOTED TO VETERAN/);
  assert.equal((await sounds()).filter((key) => key === "promotion").length, 1);
  assert.equal(await chevrons(), 1);
  await page.screenshot({ path: `${out}/promotion.png` });
  await page.evaluate(() => {
    const s = window.sloppy.sim,
      t = s.human,
      bot = s.tanks.find((b) => b.id === window.enemyId);
    t.xp = 749;
    s.damageTank(bot, 1, t.id, t.team);
    // The bot is promoted through real damage credit, nudged to reach its threshold.
    bot.xp = 1490;
    s.damageTank(t, 10, bot.id, bot.team);
  });
  await advance();
  assert.equal(await page.locator("#rank").innerText(), "ELITE");
  assert.equal(await chevrons(enemyId), 3, "Heroic bot shows three chevrons");
  await page.evaluate(() => {
    const s = window.sloppy.sim;
    s.human.xp = 1499;
    s.damageTank(
      s.tanks.find((b) => b.id === window.enemyId),
      1,
      s.human.id,
      s.human.team,
    );
  });
  await advance();
  assert.equal(await page.locator(".tank-label #rank").innerText(), "HEROIC");
  checks.push(
    "Real hit promotes with rank label, toast, chevron and chime; bot and Heroic chevrons",
  );

  // Hit confirmation follows credited hull damage, not cover, another shooter or self damage.
  await page.evaluate(() => {
    const d = window.sloppy,
      s = d.sim,
      t = s.human,
      enemy = s.tanks.find((b) => b.id === window.enemyId);
    s.shots = [];
    enemy.hp = 100;
    t.hp = 100;
    d.view.hitConfirmUntil = 0;
    d.audio.lastHit = -Infinity;
    window.soundCalls = [];
    s.damageTank(enemy, 1, -123, t.team);
    s.damageTank(t, 1, t.id, t.team);
    s.events.push({ type: "impact", x: 0, z: -6 });
  });
  await advance(1);
  assert.equal(await view("crosshair.scale.x"), 1);
  assert.ok(!(await sounds()).includes("hit"));
  await page.evaluate(() => {
    const s = window.sloppy.sim,
      t = s.human,
      enemy = s.tanks.find((b) => b.id === window.enemyId);
    t.cooldown = 0.5;
    s.damageTank(enemy, 10, t.id, t.team);
    s.damageTank(enemy, 10, t.id, t.team);
  });
  await advance(1);
  assert.equal(await view("crosshair.scale.x"), 1.2);
  assert.equal(await view("reticleInk.opacity"), 1, "a hit briefly overrides the reload dim");
  assert.equal((await sounds()).filter((key) => key === "hit").length, 1, "pellet ticks coalesce");
  await page.screenshot({ path: `${out}/confirmed-hit.png` });
  await advance(12);
  assert.equal(await view("crosshair.scale.x"), 1);
  assert.equal(await view("reticleInk.opacity"), 0.3);
  await page.evaluate(() => {
    const d = window.sloppy,
      s = d.sim,
      t = s.human;
    d.audio.lastHit = -Infinity;
    s.damageTank(
      s.tanks.find((b) => b.id === window.enemyId),
      999,
      t.id,
      t.team,
    );
  });
  await advance(1);
  assert.equal(await view("crosshair.scale.x"), 1.2);
  assert.ok((await sounds()).includes("hit"));
  // Lethal damage removed the enemy's body; keep it out of the remaining fixtures.
  await page.evaluate(() => {
    window.sloppy.sim.tanks = [window.sloppy.sim.human];
  });
  checks.push("Surviving and lethal credited hits flash and tick once; other damage does not");

  // Critical health styling below a quarter of the (rank-raised) hull, pause hints
  // and the paused status animation.
  const setHealth = (ratio) =>
    page.evaluate((ratio) => {
      const s = window.sloppy.sim;
      s.human.hp = s.maxHealth(s.human) * ratio;
    }, ratio);
  await setHealth(0.25);
  await advance();
  const critical = () =>
    page.locator(".status").evaluate((e) => e.classList.contains("critical-health"));
  const animation = () =>
    page.locator(".status").evaluate((e) => getComputedStyle(e).animationPlayState);
  assert.equal(await critical(), false);
  await setHealth(0.24);
  await advance();
  assert.equal(await critical(), true);
  await page.keyboard.press("Escape");
  await advance();
  assert.match(await page.locator(".menu").innerText(), /Q \/ E \/ scroll/);
  assert.match(await page.locator(".menu").innerText(), /Shift \+ scroll: zoom/);
  assert.equal(await animation(), "paused");
  await page.locator("#resume").click();
  await advance();
  assert.equal(await animation(), "running");
  await setHealth(1);
  await advance();
  assert.equal(await critical(), false);
  checks.push("Critical health styling, pause menu hints, paused and resumed status animation");

  // Drive into the laser pickup with the real W key.
  await page.evaluate(() => {
    const d = window.sloppy,
      s = d.sim,
      t = s.human;
    t.body.setTranslation({ x: 0, y: 0.65, z: 5 }, true);
    t.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
    t.previous = { x: 0, z: 5 };
    s.pickups = [Object.assign(window.laserPickup, { x: 0, z: 0, available: true, cooldown: 0 })];
    s.world.step();
    d.view.reset(s);
  });
  await advance();
  await page.keyboard.down("w");
  let collected = false;
  for (let frame = 0; frame < 120 && !collected; frame++) {
    await advance(1);
    collected = (await human("laser")) > 0;
  }
  await page.keyboard.up("w");
  assert.ok(collected, "W-key driving must reach the laser pickup within two seconds");
  await advance(5);
  assert.match(await page.locator("#toast").innerText(), /LASER DEFENSE/);
  assert.match(await page.locator("#effects").innerText(), /LASER DEFENSE/);
  assert.equal(await view("laserVisuals.lens.count"), 1);
  assert.deepEqual(
    await page.evaluate(() => {
      const d = window.sloppy,
        p = d.sim.pickups[0],
        g = d.view.pickupMeshes.get(p.id);
      return {
        available: p.available,
        podium: g.visible,
        gem: g.userData.gem.visible,
        refill: g.userData.refill.visible,
      };
    }),
    { available: false, podium: true, gem: false, refill: true },
  );
  // A rocket inside the laser's reach vaporizes with a beam and the saved zap.
  await page.evaluate(() => {
    const d = window.sloppy,
      s = d.sim,
      t = s.human;
    s.shots = [];
    t.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
    t.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    t.previous = { x: 0, z: 0 };
    t.laser = 6;
    s.world.step();
    window.soundCalls = [];
    window.nextRandom = s.rng.next;
    s.rng.next = () => 0; // Force the interception roll.
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
  });
  await advance(1);
  await page.evaluate(() => {
    window.sloppy.sim.rng.next = window.nextRandom;
  });
  assert.equal(await page.evaluate(() => window.sloppy.sim.shots.length), 0);
  assert.equal(await view("laserVisuals.core.count"), 1);
  assert.equal((await sounds()).filter((key) => key === "laser").length, 1);
  await page.screenshot({ path: `${out}/rocket-intercept.png` });
  await advance(9);
  assert.equal(await view("laserVisuals.core.count"), 0);
  assert.equal(
    await human("hp"),
    await page.evaluate(() => window.sloppy.sim.maxHealth(window.sloppy.sim.human)),
  );
  // Active and expiring effects must not resize the status panel.
  await page.evaluate(() => {
    const t = window.sloppy.sim.human;
    t.shield = t.rapid = t.speed = 10;
  });
  await advance();
  assert.equal(await panelHeight(), emptyPanel, "active effects keep the panel height");
  await page.evaluate(() => {
    const t = window.sloppy.sim.human;
    t.laser = t.shield = t.rapid = t.speed = 0.01;
  });
  await advance();
  assert.equal(await panelHeight(), emptyPanel, "expired effects keep the panel height");
  assert.equal(await view("laserVisuals.lens.count"), 0);
  assert.doesNotMatch(await page.locator("#effects").innerText(), /LASER/);
  checks.push("Laser pickup by W key, beam, zap and expiry; effects keep the panel height");

  // Every ordinary pickup leaves a dim podium with refill progress until it returns.
  await page.evaluate(() => {
    const d = window.sloppy,
      s = d.sim;
    s.shots = [];
    s.human.hp = s.maxHealth(s.human) * 0.24;
    s.pickups = [
      "rapid",
      "spread",
      "rocket",
      "ricochet",
      "piercing",
      "repair",
      "shield",
      "speed",
    ].map((kind, i) => ({
      id: s.nextId++,
      kind,
      x: (i - 3.5) * 4,
      z: -6,
      available: i === 7,
      cooldown: [13, 11, 9, 7, 5, 3, 1, 0][i],
      cooldownDuration: 13,
    }));
    d.view.reset(s);
  });
  await advance();
  const pads = () =>
    page.evaluate(() =>
      window.sloppy.sim.pickups.map((p) => {
        const g = window.sloppy.view.pickupMeshes.get(p.id);
        return {
          visible: g.visible,
          gem: g.userData.gem.visible,
          progress: g.userData.refill.geometry.drawRange.count,
          ring: g.userData.ring.material.opacity,
        };
      }),
    );
  const initialPads = await pads();
  assert.ok(initialPads.every((p) => p.visible));
  assert.deepEqual(
    initialPads.map((p) => p.gem),
    [false, false, false, false, false, false, false, true],
  );
  for (let i = 1; i < 7; i++) assert.ok(initialPads[i - 1].progress < initialPads[i].progress);
  assert.deepEqual(
    initialPads.map((p) => p.ring),
    [0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 1],
  );
  await page.screenshot({ path: `${out}/refill-pads.png` });
  await page.evaluate(() => {
    for (const p of window.sloppy.sim.pickups) Object.assign(p, { available: true, cooldown: 0 });
  });
  await advance();
  assert.ok((await pads()).every((p) => p.gem && p.ring === 1));
  checks.push(
    "Refill podiums: hidden gems, ordered progress, dim rings; returned pickups light up",
  );

  // A narrow window keeps the HUD on screen; death clears critical health and rank.
  await page.setViewportSize({ width: 600, height: 780 });
  await page.waitForFunction(() => window.sloppy.view.renderer.domElement.width === 600);
  await advance();
  const bounds = await page.locator(".combat-status").boundingBox();
  assert.ok(bounds.x >= 0 && bounds.x + bounds.width <= 600);
  assert.ok(await page.locator(".combat-status").evaluate((e) => e.scrollWidth <= e.clientWidth));
  assert.ok(
    await page
      .locator(".tank-label")
      .evaluate((e) => e.getBoundingClientRect().right <= innerWidth),
  );
  await page.screenshot({ path: `${out}/narrow-hud.png` });
  await page.setViewportSize({ width: 1600, height: 900 });
  await page.evaluate(() => {
    const s = window.sloppy.sim;
    s.damageTank(s.human, 999, s.human.id, s.human.team);
  });
  await advance();
  assert.equal(await critical(), false);
  await advance(185);
  assert.equal(await human("alive"), true, "the player respawns");
  assert.equal(await page.locator("#rank").innerText(), "ROOKIE");
  assert.equal(await chevrons(), 0);
  checks.push("Narrow HUD fits; death clears critical health; respawn resets rank and chevrons");

  // Every saved sound decodes and plays to completion.
  const durations = await page.evaluate(async () => {
    const result = {};
    for (const [key, sound] of Object.entries(window.sloppy.audio.sounds)) {
      await new Promise((resolve, reject) => {
        const id = sound.play();
        sound.once("end", resolve, id);
        sound.once("playerror", reject, id);
      });
      result[key] = sound.duration();
    }
    return result;
  });
  assert.deepEqual(Object.keys(durations).sort(), [
    "explosion",
    "hit",
    "impact",
    "laser",
    "pickup",
    "promotion",
    "rubble-break",
    "shot",
    "shot-piercing",
    "shot-ricochet",
    "shot-rocket",
    "shot-spread",
    "wood-break",
  ]);
  checks.push("All 13 saved sounds decode and play to completion");
  assert.deepEqual(errors, []);
  writeFileSync(
    `${out}/results.json`,
    JSON.stringify({ checks, initialPads, durations, errors }, null, 2),
  );
  console.log(JSON.stringify({ checks, errors }, null, 2));
} finally {
  await browser.close();
}
