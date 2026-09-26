import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import {
  botAssignment,
  BOT_PERSONALITIES,
  BOT_PROFILES,
  botReload,
  combatMovement,
  preferredAmmo,
  shuffledBotNames,
} from "../src/game/bot-personalities";
import { botCommand } from "../src/game/ai";
import { collectPickup, fireWeapon, weaponInterval } from "../src/game/weapons";
import { STEP, WEAPONS } from "../src/game/data";
import { clearArena } from "./fixtures";
before(async () => {
  await RAPIER.init();
});
function duel(range = 18) {
  const s = new Simulation(123);
  const human = s.human,
    bot = s.tanks.find((t) => t.team !== human.team)!;
  clearArena(s, [human, bot]);
  bot.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
  human.body.setTranslation({ x: 0, y: 0.65, z: range }, true);
  bot.aim = 0;
  bot.brain.decision = 0;
  bot.brain.ultraAggressive = false;
  s.world.step();
  s.start();
  return { s, bot, human };
}

test("all roles appear, assignment is stable, and only every tenth bot is a hunter", () => {
  const rare = Array.from({ length: 100 }, (_, i) => botAssignment(i, i % 2, i)).flatMap((b, i) =>
    b.ultraAggressive ? [i] : [],
  );
  assert.deepEqual(rare, [9, 19, 29, 39, 49, 59, 69, 79, 89, 99]);
  const s = new Simulation(123);
  const roster = s.tanks
    .filter((t) => !t.human)
    .map((t) => [t.brain.personality, t.brain.ultraAggressive]);
  assert.equal(roster.filter(([, rare]) => rare).length, 1);
  assert.equal(new Set(roster.map(([role]) => role)).size, BOT_PERSONALITIES.length);
  s.reset();
  assert.deepEqual(
    s.tanks.filter((t) => !t.human).map((t) => [t.brain.personality, t.brain.ultraAggressive]),
    roster,
  );
  s.dispose();
});

test("scouts close, guards strafe, snipers hold or retreat, and hunters pursue close range", () => {
  const { s, bot } = duel();
  bot.brain.personality = "scout";
  assert.ok(combatMovement(bot, 0, 20, 1).z > 0);
  bot.brain.personality = "guard";
  assert.ok(combatMovement(bot, 0, 18, 1).x > 0);
  bot.brain.personality = "sniper";
  assert.deepEqual(combatMovement(bot, 0, 24, 1), { x: 0, z: 0 });
  assert.ok(combatMovement(bot, 0, 10, 1).z < 0);
  bot.brain.ultraAggressive = true;
  assert.ok(combatMovement(bot, 0, 24, 1).z > 0);
  s.dispose();
});

test("hunters seek through cover but cannot shoot through it; ordinary bots require sight", () => {
  const { s, bot, human } = duel();
  s.addCover({ kind: "concrete", x: 0, z: 9, w: 20, d: 2, h: 3, hp: Infinity, color: 0 });
  s.nav.rebuild(s.covers);
  s.world.step();
  bot.brain.personality = "guard";
  botCommand(s, bot, STEP);
  assert.equal(bot.brain.target, 0);
  bot.brain.ultraAggressive = true;
  bot.brain.decision = 0;
  const command = botCommand(s, bot, STEP);
  assert.equal(bot.brain.target, human.id);
  assert.equal(command.fire, false);
  assert.ok(bot.brain.path.length > 0);
  s.dispose();
});

test("snipers actually hold their firing lane and minelayers deliberately deploy near enemies", () => {
  const { s, bot } = duel(24);
  bot.brain.personality = "sniper";
  const hold = botCommand(s, bot, STEP);
  assert.ok(Math.hypot(hold.moveX, hold.moveZ) < 0.01);
  assert.equal(hold.fire, false, "acquiring a target still requires reaction time");
  bot.brain.personality = "minelayer";
  s.human.body.setTranslation({ x: 0, y: 0.65, z: 12 }, true);
  s.world.step();
  bot.brain.decision = 0;
  assert.equal(botCommand(s, bot, STEP).mine, true);
  bot.mineCooldown = 1;
  assert.equal(botCommand(s, bot, STEP).mine, false);
  s.dispose();
});

test("artillery needs crates and every role preserves the player cadence edge", () => {
  const { s, bot, human } = duel();
  bot.brain.personality = "artillery";
  fireWeapon(s, bot);
  assert.equal(s.shots.at(-1)!.weapon, "standard");
  collectPickup(s, bot, { id: 9999, x: 0, z: 0, kind: "rocket", available: true, cooldown: 0 });
  assert.equal(bot.selectedAmmo, "standard");
  bot.selectedAmmo = preferredAmmo(bot);
  bot.cooldown = 0;
  fireWeapon(s, bot);
  assert.equal(s.shots.at(-1)!.weapon, "rocket");
  assert.equal(bot.ammo.rocket, 11);
  for (const role of BOT_PERSONALITIES)
    for (const ultra of [false, true])
      for (const rapid of [0, 12]) {
        bot.brain.personality = role;
        bot.brain.ultraAggressive = ultra;
        for (const weapon of ["standard", "spread", "rocket", "ricochet", "piercing"] as const) {
          bot.selectedAmmo = human.selectedAmmo = weapon;
          if (weapon !== "standard") bot.ammo[weapon] = human.ammo[weapon] = 5;
          bot.rapid = human.rapid = rapid;
          assert.ok(botReload(bot, 0) > weaponInterval(human) * 1.2, `${role} ${weapon} cadence`);
        }
      }
  assert.ok(BOT_PROFILES.artillery.reload > BOT_PROFILES.sniper.reload);
  s.dispose();
});

test("bots pause between shots even when breaching; the human fires faster with and without rapid fire", () => {
  for (const rapid of [false, true]) {
    const { s, bot, human } = duel();
    try {
      bot.body.setTranslation({ x: -8, y: 0.65, z: 0 }, true);
      human.body.setTranslation({ x: -30, y: 0.65, z: 30 }, true);
      s.world.step();
      bot.aim = Math.PI / 2;
      bot.rapid = human.rapid = rapid ? 12 : 0;
      bot.brain.goal = { x: 4, z: 0 };
      bot.brain.decision = bot.brain.memory = 100;
      s.addCover({ kind: "concrete", x: 0, z: 0, w: 1, d: 5, h: 2, hp: 10000, color: 0 });
      let botShots = 0,
        humanShots = 0;
      for (let i = 0; i < 600; i++) {
        bot.cooldown = Math.max(0, bot.cooldown - STEP);
        human.cooldown = Math.max(0, human.cooldown - STEP);
        if (botCommand(s, bot, STEP).fire && bot.cooldown === 0) {
          fireWeapon(s, bot);
          botShots++;
        }
        if (human.cooldown === 0) {
          fireWeapon(s, human);
          humanShots++;
        }
      }
      assert.ok(
        botShots > 0 && botShots < humanShots * 0.8,
        JSON.stringify({ rapid, botShots, humanShots }),
      );
      const interval = (WEAPONS.standard.interval * (rapid ? 0.5 : 1)) / 1.2;
      assert.ok(humanShots >= Math.floor(10 / (interval + STEP)));
    } finally {
      s.dispose();
    }
  }
});

test("bot name decks are derived from the round seed: unique, stable per seed, new each round, kept on respawn", () => {
  const deck = shuffledBotNames(123);
  assert.ok(deck.length >= 80);
  assert.equal(new Set(deck).size, deck.length);
  assert.deepEqual(deck, shuffledBotNames(123));
  assert.notDeepEqual(deck, shuffledBotNames(124));
  const s = new Simulation(123),
    t = s.tanks.find((t) => !t.human)!;
  const initial = s.tanks.filter((t) => !t.human).map((t) => t.name),
    name = t.name;
  assert.equal(new Set(initial).size, initial.length);
  t.protection = 0;
  s.damageTank(t, 1000, 999, (1 - t.team) as 0 | 1);
  s.respawn(t);
  assert.equal(t.name, name);
  s.reset();
  assert.notDeepEqual(
    s.tanks.filter((t) => !t.human).map((t) => t.name),
    initial,
  );
  s.dispose();
});
