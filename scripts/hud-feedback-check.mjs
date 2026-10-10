// HUD, reticle, rank, laser and pickup feedback with real mouse, wheel and keyboard
// input, the real game loop at fixed frame steps, and the saved sounds. Rules behind
// this feedback (selection, XP, laser timing, refills) are covered by the engine's
// tests; fixtures are arranged through the dev-only `Game.debug_*` hooks.
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { freezeLoop, gameUrl, launchGame, startRound } from "./browser-helpers.mjs";

const out = "artifacts/performance/hud-feedback";
mkdirSync(out, { recursive: true });
const { browser, page, errors } = await launchGame();
const checks = [];
/** A shooter id that is no tank: damage it deals credits nobody. */
const NOBODY = 999999;
const PICKUPS = ["rapid", "spread", "rocket", "ricochet", "piercing", "repair", "shield", "speed"];
try {
  await freezeLoop(page);
  await page.goto(gameUrl);
  await startRound(page);
  // HUD feedback must consume elapsed time, including frames between HUD refreshes.
  const hudTiming = await page.evaluate(async () => {
    // Vite can timestamp this import after a rebuild; patch the class the page loaded.
    const uiModule = performance
      .getEntriesByType("resource")
      .findLast((entry) => new URL(entry.name).pathname.endsWith("/src/game/ui.ts"));
    if (!uiModule) throw new Error("The page did not load the HUD module");
    const { UI } = await import(uiModule.name);
    const { FRAME } = await import(new URL("src/game/engine-api.ts", location.href).href);
    const originalUpdate = UI.prototype.update;
    const game = window.sloppy.game;
    const originalFrame = game.frame;
    let frameSeconds = 0;
    let hudSeconds = 0;
    let hudDue = false;
    game.frame = function (...args) {
      const result = originalFrame.apply(this, args);
      frameSeconds += result[FRAME.dt];
      hudDue = result[FRAME.hudDue] === 1;
      return result;
    };
    UI.prototype.update = function (state, dt) {
      hudSeconds += dt;
      return originalUpdate.call(this, state, dt);
    };
    try {
      do {
        window.advanceFrame(16);
      } while (!hudDue);
      frameSeconds = hudSeconds = 0;
      for (const ms of [16, 16, 16, 100, 16, 16, 16, 100]) window.advanceFrame(ms);
      return { frameSeconds, hudSeconds };
    } finally {
      game.frame = originalFrame;
      UI.prototype.update = originalUpdate;
    }
  });
  assert.ok(Math.abs(hudTiming.frameSeconds - 0.296) < 0.00001);
  assert.ok(
    Math.abs(hudTiming.hudSeconds - hudTiming.frameSeconds) < 0.00001,
    JSON.stringify(hudTiming),
  );
  checks.push("HUD feedback follows 296 ms of uneven frames, including both 100 ms frames");

  // One cleared arena: the player at the origin facing a frozen enemy 7 m north.
  const ids = await page.evaluate(() => {
    const { sloppy, engine } = window;
    const game = sloppy.game;
    sloppy.autoplay(false);
    const { human, tanks } = sloppy.sim;
    const enemy = tanks.find((tank) => tank.team !== human.team);
    game.debug_clear_arena(new Uint32Array([enemy.id]));
    game.debug_place_tank(human.id, 0, 0, NaN);
    game.debug_place_tank(enemy.id, 0, -7, NaN);
    engine.setTank(human.id, {
      protection: 0,
      ammo: { spread: 18, rocket: 12, ricochet: 0, piercing: 24 },
    });
    engine.setTank(enemy.id, { protection: 0, frozen: true });
    sloppy.zoom(23);
    window.soundCalls = [];
    for (const [key, sound] of Object.entries(sloppy.audio.sounds)) {
      const play = sound.play.bind(sound);
      sound.play = (...args) => {
        // Howler also calls play(id, true) internally when setting spatial state.
        if (args.length === 0) window.soundCalls.push(key);
        return play(...args);
      };
    }
    window.advanceFrame(17);
    return { human: human.id, humanTeam: human.team, enemy: enemy.id, enemyTeam: enemy.team };
  });
  // The HUD refreshes every fourth rendered frame.
  const advance = (count = 4) =>
    page.evaluate((n) => {
      for (let i = 0; i < n; i++) window.advanceFrame(17);
    }, count);
  const human = (key) => page.evaluate((key) => window.sloppy.sim.human[key], key);
  const reticle = () => page.evaluate(() => window.engine.view().reticle);
  const setHuman = (patch) => page.evaluate((patch) => window.engine.setHuman(patch), patch);
  const damage = (id, amount, owner, team) =>
    page.evaluate(
      ([id, amount, owner, team]) => window.sloppy.game.debug_damage_tank(id, amount, owner, team),
      [id, amount, owner, team],
    );
  const sounds = () => page.evaluate(() => window.soundCalls.splice(0));
  const panelHeight = () => page.locator(".combat-status").evaluate((e) => e.offsetHeight);
  const chevrons = (id = ids.human) =>
    page.evaluate((id) => window.engine.view().tanks.find((tank) => tank.id === id).chevrons, id);
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
  await page.waitForFunction(() => window.sloppy.controls.wheelAmmo === 1);
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
  // Reloading shows the dim reticle until the cannon is ready.
  await setHuman({ cooldown: 0.6 });
  await advance();
  assert.deepEqual(await reticle().then(({ ready, reloading }) => ({ ready, reloading })), {
    ready: false,
    reloading: true,
  });
  await advance(40);
  assert.deepEqual(await reticle().then(({ ready, reloading }) => ({ ready, reloading })), {
    ready: true,
    reloading: false,
  });
  // The production event route plays each actual shot's weapon sound.
  for (const [weapon, sound] of [
    ["standard", "shot"],
    ["spread", "shot-spread"],
    ["rocket", "shot-rocket"],
    ["ricochet", "shot-ricochet"],
    ["piercing", "shot-piercing"],
  ]) {
    await page.evaluate((weapon) => {
      window.engine.setHuman({
        ammo: { spread: 2, rocket: 2, ricochet: 2, piercing: 2 },
        selectedAmmo: weapon,
        cooldown: 0,
      });
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
    window.sloppy.game.debug_clear_shots();
    window.engine.setHuman({ selectedAmmo: "standard", cooldown: 0, xp: 280 });
    window.soundCalls = [];
  });
  const [targetX, targetY] = await page.evaluate(() =>
    Array.from(window.sloppy.game.debug_screen_point(0, 0, -7)),
  );
  await page.mouse.move(targetX, targetY);
  await page.mouse.down();
  await advance(24);
  await page.mouse.up();
  assert.equal(await page.locator("#rank").innerText(), "VETERAN");
  assert.equal(await page.locator("#xp, #xpbar, .veterancy").count(), 0);
  assert.match(await page.locator("#toast").innerText(), /PROMOTED TO VETERAN/);
  assert.equal((await sounds()).filter((key) => key === "promotion").length, 1);
  assert.equal(await chevrons(), 1);
  await page.screenshot({ path: `${out}/promotion.png` });
  // The bot is promoted through real damage credit, nudged to reach its threshold.
  await setHuman({ xp: 749 });
  await damage(ids.enemy, 1, ids.human, ids.humanTeam);
  await page.evaluate((id) => window.engine.setTank(id, { xp: 1490 }), ids.enemy);
  await damage(ids.human, 10, ids.enemy, ids.enemyTeam);
  await advance();
  assert.equal(await page.locator("#rank").innerText(), "ELITE");
  assert.equal(await chevrons(ids.enemy), 3, "Heroic bot shows three chevrons");
  await setHuman({ xp: 1499 });
  await damage(ids.enemy, 1, ids.human, ids.humanTeam);
  await advance();
  assert.equal(await page.locator(".tank-label #rank").innerText(), "HEROIC");
  checks.push(
    "Real hit promotes with rank label, toast, chevron and chime; bot and Heroic chevrons",
  );

  // Hit confirmation follows credited hull damage, not another shooter or self damage.
  await advance(20); // Let the promotion hit's confirmation expire.
  await page.evaluate(
    ({ enemy }) => {
      window.sloppy.game.debug_clear_shots();
      window.engine.setTank(enemy, { hp: 100 });
      window.engine.setHuman({ hp: 100 });
      window.sloppy.audio.lastHit = -Infinity;
      window.soundCalls = [];
    },
    { enemy: ids.enemy },
  );
  await damage(ids.enemy, 1, NOBODY, ids.humanTeam);
  await damage(ids.human, 1, ids.human, ids.humanTeam);
  await advance(1);
  assert.equal((await reticle()).confirmed, false);
  assert.equal((await reticle()).scale, 1);
  assert.ok(!(await sounds()).includes("hit"));
  await setHuman({ cooldown: 0.5 });
  await damage(ids.enemy, 10, ids.human, ids.humanTeam);
  await damage(ids.enemy, 10, ids.human, ids.humanTeam);
  await advance(1);
  let shown = await reticle();
  assert.ok(Math.abs(shown.scale - 1.2) < 1e-6, `confirmed hit scale ${shown.scale}`);
  assert.deepEqual(
    [shown.confirmed, shown.reloading],
    [true, false],
    "a hit briefly overrides the reload ring",
  );
  assert.equal((await sounds()).filter((key) => key === "hit").length, 1, "pellet ticks coalesce");
  await page.screenshot({ path: `${out}/confirmed-hit.png` });
  await advance(12);
  shown = await reticle();
  assert.deepEqual([shown.scale, shown.confirmed, shown.reloading], [1, false, true]);
  await page.evaluate(() => {
    window.sloppy.audio.lastHit = -Infinity;
  });
  await damage(ids.enemy, 999, ids.human, ids.humanTeam);
  await advance(1);
  assert.ok(Math.abs((await reticle()).scale - 1.2) < 1e-6);
  assert.ok((await sounds()).includes("hit"));
  // Lethal damage removed the enemy's body; keep it out of the remaining fixtures.
  await page.evaluate(() => window.sloppy.game.debug_clear_arena(new Uint32Array()));
  checks.push("Surviving and lethal credited hits flash and tick once; other damage does not");

  // Critical health styling below a quarter of the (rank-raised) hull, pause hints
  // and the paused status animation.
  const setHealth = (ratio) =>
    page.evaluate((ratio) => {
      const { maxHp } = window.sloppy.hud().human;
      window.engine.setHuman({ hp: maxHp * ratio });
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
  await page.evaluate((id) => {
    const game = window.sloppy.game;
    game.debug_place_tank(id, 0, 5, NaN);
    game.debug_set_pickups(JSON.stringify([{ kind: "laser", x: 0, z: 0, available: true }]));
  }, ids.human);
  await advance();
  await page.keyboard.down("w");
  let collected = false;
  for (let frame = 0; frame < 120 && !collected; frame++) {
    await advance(1);
    collected = (await page.evaluate(() => window.sloppy.hud().human.laser)) > 0;
  }
  await page.keyboard.up("w");
  assert.ok(collected, "W-key driving must reach the laser pickup within two seconds");
  await advance(5);
  assert.match(await page.locator("#toast").innerText(), /LASER DEFENSE/);
  assert.match(await page.locator("#effects").innerText(), /LASER DEFENSE/);
  const collectedView = await page.evaluate(() => window.engine.view());
  assert.equal(collectedView.laser.lenses, 1);
  const podium = collectedView.pickups[0];
  assert.deepEqual(
    { base: podium.baseShown, gem: podium.gem, refill: podium.refill },
    { base: true, gem: false, refill: true },
  );
  // A rocket inside the laser's reach vaporizes with a beam and the saved zap.
  await page.evaluate(
    ({ id, enemyTeam, nobody }) => {
      const game = window.sloppy.game;
      game.debug_clear_shots();
      game.debug_place_tank(id, 0, 0, NaN);
      window.engine.setHuman({ laser: 6 });
      window.soundCalls = [];
      game.debug_rig_rng(0.01); // Win the interception roll.
      game.debug_add_shot(
        JSON.stringify({
          x: -4,
          z: -4,
          vx: 12,
          vz: 12,
          team: enemyTeam,
          owner: nobody,
          weapon: "rocket",
          damage: 65,
          life: 3.5,
        }),
      );
    },
    { id: ids.human, enemyTeam: ids.enemyTeam, nobody: NOBODY },
  );
  await advance(1);
  assert.equal(await page.evaluate(() => window.sloppy.sim.shots), 0);
  assert.equal((await page.evaluate(() => window.engine.view())).laser.cores, 1);
  assert.equal((await sounds()).filter((key) => key === "laser").length, 1);
  await page.screenshot({ path: `${out}/rocket-intercept.png` });
  await advance(9);
  assert.equal((await page.evaluate(() => window.engine.view())).laser.cores, 0);
  assert.equal(await human("hp"), await page.evaluate(() => window.sloppy.hud().human.maxHp));
  // Active and expiring effects must not resize the status panel.
  await setHuman({ shield: 10, rapid: 10, speed: 10 });
  await advance();
  assert.equal(await panelHeight(), emptyPanel, "active effects keep the panel height");
  await setHuman({ laser: 0.01, shield: 0.01, rapid: 0.01, speed: 0.01 });
  await advance();
  assert.equal(await panelHeight(), emptyPanel, "expired effects keep the panel height");
  assert.equal((await page.evaluate(() => window.engine.view())).laser.lenses, 0);
  assert.doesNotMatch(await page.locator("#effects").innerText(), /LASER/);
  checks.push("Laser pickup by W key, beam, zap and expiry; effects keep the panel height");

  // Every ordinary pickup leaves a dim podium with refill progress until it returns.
  await page.evaluate((kinds) => {
    const game = window.sloppy.game;
    game.debug_clear_shots();
    window.engine.setHuman({ hp: window.sloppy.hud().human.maxHp * 0.24 });
    const cooldowns = [13, 11, 9, 7, 5, 3, 1, 0];
    game.debug_set_pickups(
      JSON.stringify(
        kinds.map((kind, i) => ({
          kind,
          x: (i - 3.5) * 4,
          z: -6,
          available: i === 7,
          cooldown: cooldowns[i],
          cooldownDuration: 13,
        })),
      ),
    );
  }, PICKUPS);
  await advance();
  const pads = () =>
    page.evaluate(() =>
      window.engine.view().pickups.map((pad) => ({
        visible: pad.baseShown,
        gem: pad.gem,
        progress: pad.segments,
        lit: pad.ring && !pad.ringDim,
      })),
    );
  const initialPads = await pads();
  assert.equal(initialPads.length, 8);
  assert.ok(initialPads.every((p) => p.visible));
  assert.deepEqual(
    initialPads.map((p) => p.gem),
    [false, false, false, false, false, false, false, true],
  );
  for (let i = 1; i < 7; i++) assert.ok(initialPads[i - 1].progress < initialPads[i].progress);
  assert.deepEqual(
    initialPads.map((p) => p.lit),
    [false, false, false, false, false, false, false, true],
  );
  await page.screenshot({ path: `${out}/refill-pads.png` });
  await page.evaluate((kinds) => {
    const game = window.sloppy.game;
    game.debug_set_pickups(
      JSON.stringify(kinds.map((kind, i) => ({ kind, x: (i - 3.5) * 4, z: -6, available: true }))),
    );
  }, PICKUPS);
  await advance();
  assert.ok((await pads()).every((p) => p.gem && p.lit));
  checks.push(
    "Refill podiums: hidden gems, ordered progress, dim rings; returned pickups light up",
  );

  // A narrow window keeps the HUD on screen; death clears critical health and rank.
  await page.setViewportSize({ width: 600, height: 780 });
  await page.waitForFunction(() => window.sloppy.view.canvas[0] === 600);
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
  await damage(ids.human, 999, ids.human, ids.humanTeam);
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
  assert.equal(await page.evaluate(() => window.sloppy.error()), null, "no GPU error");
  assert.deepEqual(errors, []);
  writeFileSync(
    `${out}/results.json`,
    JSON.stringify({ checks, initialPads, durations, errors }, null, 2),
  );
  console.log(JSON.stringify({ checks, errors }, null, 2));
} finally {
  await browser.close();
}
