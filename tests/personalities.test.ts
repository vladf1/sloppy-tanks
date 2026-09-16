import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { InstancedBufferAttribute } from "three";
import { Simulation } from "../src/game/simulation";
import {
  botAssignment,
  BOT_PERSONALITIES,
  BOT_PROFILES,
  botReload,
  combatMovement,
  preferredAmmo,
} from "../src/game/bot-personalities";
import { botCommand } from "../src/game/ai";
import { collectPickup, fireWeapon, weaponInterval } from "../src/game/weapons";
import { STEP } from "../src/game/data";
import { TrackTrails, TRACK_LIFETIME, TRACK_CAPACITY } from "../src/game/tracks";
before(async () => {
  await RAPIER.init();
});
function duel(range = 18) {
  const s = new Simulation(123);
  const human = s.human,
    bot = s.tanks.find((t) => t.team !== human.team)!;
  for (const t of s.tanks) if (t !== human && t !== bot) s.world.removeRigidBody(t.body);
  s.tanks = [human, bot];
  for (const c of s.covers) s.world.removeRigidBody(c.body);
  s.covers = [];
  s.pickups = [];
  s.nav.rebuild([]);
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

test("a full track buffer cannot replace still-visible marks and can reuse fully faded ones", () => {
  const { s, bot, human } = duel();
  bot.alive = false;
  const trails = new TrackTrails();
  const fillSteps = TRACK_CAPACITY / 2;
  for (let i = 0; i < fillSteps; i++) {
    human.body.setTranslation({ x: i, y: 0.65, z: 0 }, true);
    trails.update(s, 1);
  }
  assert.equal(trails.mesh.count, TRACK_CAPACITY);
  const before = trails.mesh.instanceMatrix.array.slice();
  for (let i = fillSteps; i < fillSteps + 10; i++) {
    human.body.setTranslation({ x: i, y: 0.65, z: 0 }, true);
    trails.update(s, 1);
  }
  assert.deepEqual(trails.mesh.instanceMatrix.array, before);
  s.elapsed = TRACK_LIFETIME + 0.1;
  human.body.setTranslation({ x: fillSteps + 10, y: 0.65, z: 0 }, true);
  trails.update(s, 1);
  assert.notDeepEqual(trails.mesh.instanceMatrix.array, before);
  trails.dispose();
  s.dispose();
});

test("track expiry compacts live marks and uploads only changed slots", () => {
  const { s, bot, human } = duel();
  bot.alive = false;
  const trails = new TrackTrails();
  const matrix = trails.mesh.instanceMatrix;
  const birth = trails.mesh.geometry.getAttribute("trackBirth");
  assert.ok(birth instanceof InstancedBufferAttribute);
  human.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
  trails.update(s, 1);
  human.body.setTranslation({ x: 1, y: 0.65, z: 0 }, true);
  trails.update(s, 1);
  const oldCount = trails.mesh.count;
  s.elapsed = 10;
  human.body.setTranslation({ x: 2, y: 0.65, z: 0 }, true);
  trails.update(s, 1);
  const liveCount = trails.mesh.count - oldCount;
  const expected = [];
  for (let i = oldCount; i < trails.mesh.count; i++) {
    expected.push(Array.from(matrix.array.slice(i * 16, (i + 1) * 16)).join(","));
  }
  matrix.clearUpdateRanges();
  birth.clearUpdateRanges();
  s.elapsed = TRACK_LIFETIME - 0.001;
  trails.update(s, 1);
  assert.equal(trails.mesh.count, oldCount + liveCount, "keep marks until completely faded");
  s.elapsed = TRACK_LIFETIME;
  trails.update(s, 1);
  assert.equal(trails.mesh.count, liveCount);
  const actual = [];
  for (let i = 0; i < trails.mesh.count; i++) {
    actual.push(Array.from(matrix.array.slice(i * 16, (i + 1) * 16)).join(","));
    assert.equal(birth.getX(i), 10, "moving a slot preserves its fade age");
  }
  assert.deepEqual(actual.sort(), expected.sort(), "expiry preserves every younger mark");
  assert.ok(matrix.updateRanges.reduce((n, r) => n + r.count, 0) <= oldCount * 16);
  assert.deepEqual(
    matrix.updateRanges.map((r) => ({ start: r.start / 16, count: r.count / 16 })),
    birth.updateRanges,
  );
  s.elapsed = 10 + TRACK_LIFETIME;
  trails.update(s, 1);
  assert.equal(trails.mesh.count, 0, "no expired instances remain in the draw");
  human.body.setTranslation({ x: 3, y: 0.65, z: 0 }, true);
  trails.update(s, 1);
  assert.ok(trails.mesh.count > 0, "new tracks resume after all previous marks expire");
  trails.reset();
  assert.equal(matrix.updateRanges.length, 0);
  assert.equal(birth.updateRanges.length, 0);
  trails.dispose();
  s.dispose();
});
