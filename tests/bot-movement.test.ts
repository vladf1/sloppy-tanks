import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { idleCommand, type Tank } from "../src/game/types";
import { distance, STEP } from "../src/game/data";
import { botCommand } from "../src/game/ai";
import { routeDirection } from "../src/game/bot-movement";
import { tuneSpeed } from "../src/game/speed-tuning";
before(async () => { await RAPIER.init(); });
function arena() {
  const s = new Simulation(123);
  for (const c of s.covers) s.world.removeRigidBody(c.body);
  for (const t of s.tanks) s.world.removeRigidBody(t.body);
  s.covers = []; s.coverByCollider.clear(); s.tanks = []; s.pickups = []; s.nav.rebuild([]);
  return s;
}
function place(t: Tank, x: number, z: number) {
  t.body.setTranslation({ x, y: 0.65, z }, true); t.previous = { x, z };
  t.brain.last = { x, z }; t.protection = 1000;
}
function run(s: Simulation, t: Tank, seconds: number) {
  s.world.step(); s.start();
  let reversals = 0, last = { x: 0, z: 0 };
  for (let i = 0; i < seconds * 60; i++) {
    s.step(idleCommand());
    const c = t.command, d = Math.hypot(c.moveX, c.moveZ);
    if (d > 0.15) {
      const next = { x: c.moveX / d, z: c.moveZ / d };
      if (next.x * last.x + next.z * last.z < -0.5) reversals++;
      last = next;
    }
    s.events = [];
  }
  return reversals;
}

test("bots brake and settle near a destination without repeated direction flips, including double speed", () => {
  for (const personality of ["scout", "guard", "heavy"] as const) for (const speed of [1, 2]) {
    const s = arena(), bot = s.addTank(0, false, "balanced", personality === "heavy" ? 3 : 1);
    bot.brain.personality = personality; place(bot, 0, 0);
    bot.brain.decision = 999; bot.brain.goal = { x: 0, z: 0.7 };
    try {
      tuneSpeed(s, "tank-speed", speed);
      assert.ok(run(s, bot, 4) <= 1);
      assert.ok(distance(bot.body.translation(), bot.brain.goal) < 0.3);
      assert.ok(Math.hypot(bot.command.moveX, bot.command.moveZ) < 0.01);
    } finally { tuneSpeed(s, "tank-speed", 1); s.dispose(); }
  }
});

test("a retreating guard skirts a wall instead of alternating attack and retreat", () => {
  const s = arena(), bot = s.addTank(0, false, "balanced", 1), enemy = s.addTank(1, true, "balanced");
  place(bot, 0, -8); place(enemy, 0, 0);
  s.addCover({ kind: "concrete", x: 0, z: -12, w: 16, d: 2, h: 3, hp: Infinity, color: 0 });
  s.nav.rebuild(s.covers);
  assert.ok(run(s, bot, 4) < 5);
  assert.ok(Math.abs(bot.body.translation().x) > 8, "escape past the edge of the wall");
  s.dispose();
});

test("head-on allies pass each other and both reach their destinations", () => {
  const s = arena(), a = s.addTank(0, false, "balanced", 1), b = s.addTank(0, false, "balanced", 1);
  place(a, 0, -6); place(b, 0, 6);
  for (const [t, z] of [[a, 16], [b, -16]] as const) {
    t.brain.decision = 999; t.brain.goal = { x: 0, z };
    t.brain.path = s.nav.find(t.previous, t.brain.goal);
  }
  assert.ok(run(s, a, 10) < 5);
  assert.ok(distance(a.body.translation(), a.brain.goal) < 0.5);
  assert.ok(distance(b.body.translation(), b.brain.goal) < 0.5); s.dispose();
});

test("a stalled bot commits to its recovery route across combat decisions and clears it on respawn", () => {
  const s = arena(), bot = s.addTank(0, false, "balanced", 1), enemy = s.addTank(1, true, "balanced");
  place(bot, 0, 0); place(enemy, 0, 10); s.world.step(); s.start();
  bot.brain.stuck = 1.3; bot.brain.decision = 0;
  botCommand(s, bot, STEP);
  assert.equal(bot.brain.recoveries, 1); assert.ok(bot.brain.recovery > 1);
  const goal = { ...bot.brain.recoveryGoal };
  for (let i = 0; i < 5; i++) {
    bot.brain.decision = 0; s.step();
    assert.deepEqual(bot.brain.recoveryGoal, goal);
    assert.equal(bot.brain.recoveries, 1);
    assert.ok(distance(bot.brain.path.at(-1)!, goal) < 1.2);
  }
  assert.equal(s.snapshot().tanks[0].recovering, true);
  bot.protection = 0; s.damageTank(bot, 999, enemy.id, enemy.team); s.respawn(bot);
  assert.equal(bot.brain.recovery, 0); assert.equal(bot.brain.avoidanceTime, 0);
  assert.equal(bot.brain.stuck, 0); assert.equal(bot.brain.recoveries, 0); s.dispose();
});

test("bots retain comparable visible targets but react to a substantially closer enemy", () => {
  const s = arena(), bot = s.addTank(0, false, "balanced", 1);
  const a = s.addTank(1, true, "balanced"), b = s.addTank(1, true, "balanced");
  place(bot, 0, 0); place(a, 2, 18); place(b, -2, 19); s.world.step();
  bot.brain.decision = 0; botCommand(s, bot, STEP); assert.equal(bot.brain.target, a.id);
  place(b, -2, 17); s.world.step(); bot.brain.decision = 0;
  botCommand(s, bot, STEP); assert.equal(bot.brain.target, a.id);
  place(b, -2, 9); s.world.step(); bot.brain.decision = 0;
  botCommand(s, bot, STEP); assert.equal(bot.brain.target, b.id); s.dispose();
});

test("pickup and patrol destinations persist across decisions and unavailable crates are abandoned", () => {
  const s = arena(), bot = s.addTank(0, false, "balanced", 1); place(bot, 20, -38); s.world.step();
  bot.brain.goal = { x: 20, z: -38 }; bot.brain.decision = 0;
  botCommand(s, bot, STEP); const patrol = { ...bot.brain.goal };
  assert.equal(patrol.x, 46);
  for (let i = 0; i < 5; i++) {
    bot.brain.decision = 0; botCommand(s, bot, STEP); assert.deepEqual(bot.brain.goal, patrol);
  }
  s.pickups = [{ id: 999, kind: "rocket", available: true, cooldown: 0, x: 24, z: -38 },
    { id: 1000, kind: "piercing", available: true, cooldown: 0, x: 15, z: -38 }];
  bot.brain.decision = 0; botCommand(s, bot, STEP); assert.equal(bot.brain.pickupTarget, 999);
  s.pickups[1].x = 17; bot.brain.decision = 0; botCommand(s, bot, STEP);
  assert.equal(bot.brain.pickupTarget, 999);
  s.pickups[0].available = false; bot.brain.decision = 0; botCommand(s, bot, STEP);
  assert.equal(bot.brain.pickupTarget, 1000); s.dispose();
});

test("path lookahead cannot shortcut a wall and unreachable goals never become direct movement", () => {
  const s = arena(), bot = s.addTank(0, false, "balanced", 1); place(bot, 0, -5);
  s.addCover({ kind: "concrete", x: 0, z: 0, w: 10, d: 2, h: 3, hp: Infinity, color: 0 });
  s.nav.rebuild(s.covers); s.world.step();
  const goal = { x: 0, z: 5 };
  assert.equal(s.nav.clearLine(bot.previous, goal), false);
  bot.brain.goal = goal; bot.brain.path = [];
  assert.deepEqual(routeDirection(s, bot), { x: 0, z: 0 });
  bot.brain.path = s.nav.find(bot.previous, goal);
  const direction = routeDirection(s, bot);
  assert.ok(Math.abs(direction.x) > 0.8, "route around the wall rather than through it"); s.dispose();
});
