import { test, before } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { newMatch, awardKill, tickMatch } from "../src/game/match";
import { fireWeapon, stepProjectiles } from "../src/game/weapons";
import { STEP, VEHICLES, Random } from "../src/game/data";
import { idleCommand, type Team } from "../src/game/types";
import { botCommand, friendlyBlocksShot } from "../src/game/ai";
import { Navigation } from "../src/game/navigation";
before(async () => {
  await RAPIER.init();
});
test("reused navigation searches recover from unreachable goals and changed topology", () => {
  const nav = new Navigation(),
    from = { x: -40, z: -40 },
    to = { x: 40, z: 40 };
  const first = nav.find(from, to),
    saved = structuredClone(first);
  nav.blocked.fill(1);
  assert.deepEqual(nav.find(from, to), []);
  nav.rebuild([]);
  assert.deepEqual(nav.find(to, from), new Navigation().find(to, from));
  assert.deepEqual(nav.find(from, to), saved);
  assert.deepEqual(nav.find(from, from), []);
  assert.deepEqual(first, saved, "later searches cannot mutate a bot's existing path");
});

function game() {
  const s = new Simulation(123);
  s.start();
  for (const t of s.tanks) t.protection = 0;
  return s;
}
function place(s: Simulation, id: number, x: number, z: number) {
  const t = s.tanks[id];
  t.body.setTranslation({ x, y: 0.65, z }, true);
  t.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
  t.previous = { x, z };
  s.world.step();
  return t;
}
function clear(s: Simulation) {
  for (const c of s.covers)
    if (c.kind !== "boundary") {
      c.alive = false;
      s.world.removeRigidBody(c.body);
    }
  s.nav.rebuild(s.covers);
  for (let i = 0; i < s.tanks.length; i++) place(s, i, -30 + i * 3, 30);
}
test("few-hit combat, friendly safety, exactly-once kills and attribution", () => {
  const s = game(),
    a = s.tanks[0],
    b = s.tanks[1];
  s.damageTank(a, 40, s.tanks[2].id, 0);
  assert.equal(a.hp, VEHICLES[a.kind].health);
  s.damageTank(b, 1000, a.id, 0);
  assert.equal(s.match.scores[0], 1);
  s.damageTank(b, 1000, a.id, 0);
  assert.equal(s.match.scores[0], 1);
  assert.equal(a.kills, 1);
  s.dispose();
});
test("self explosion damages owner and self-kill awards no point", () => {
  const s = game(),
    a = s.tanks[0];
  s.explode(a.body.translation(), 6, 1000, a.id, a.team);
  assert.equal(a.alive, false);
  assert.deepEqual(s.match.scores, [0, 0]);
  s.dispose();
});
test("protection prevents damage, expires after 2 seconds, firing cancels it", () => {
  const s = game(),
    a = s.human;
  s.human.protection = 2;
  s.damageTank(a, 1000, 999, (1 - a.team) as Team);
  assert.ok(a.alive);
  fireWeapon(s, a);
  assert.equal(a.protection, 0);
  a.protection = 2;
  for (let i = 0; i < 121; i++) s.step();
  assert.equal(a.protection, 0);
  s.dispose();
});
test("respawn occurs after three seconds with protection and selected class", () => {
  const s = game(),
    a = s.human;
  s.damageTank(a, 1000, 999, (1 - a.team) as Team);
  s.humanKind = "heavy";
  for (let i = 0; i < 179; i++) s.step();
  assert.equal(a.alive, false);
  for (let i = 0; i < 3; i++) s.step();
  assert.ok(a.alive);
  assert.equal(a.hp, 140);
  assert.ok(a.protection > 1.9);
  s.dispose();
});
test("drum chain kills keep the initiating team and self-kills do not score", () => {
  const s = game();
  clear(s);
  const a = place(s, 0, -20, 0),
    b = place(s, 1, 6, 0);
  b.hp = 20;
  const d1 = s.addCover({
      kind: "drum",
      x: 0,
      z: 0,
      w: 1,
      d: 1,
      h: 2,
      hp: 30,
      color: 0,
    }),
    d2 = s.addCover({
      kind: "drum",
      x: 4,
      z: 0,
      w: 1,
      d: 1,
      h: 2,
      hp: 30,
      color: 0,
    });
  s.damageCover(d1, 40, a.id, a.team);
  assert.equal(d2.alive, false);
  assert.equal(b.alive, false);
  assert.equal(a.kills, 1);
  assert.equal(s.destroyed, 2);
  s.dispose();
});
test("match time, tie overtime, next valid kill and 100 kill limit", () => {
  const m = newMatch();
  m.phase = "playing";
  m.time = STEP;
  tickMatch(m, STEP);
  assert.ok(m.overtime);
  awardKill(m, 0, 0, true);
  assert.equal(m.phase, "playing");
  awardKill(m, 0, 1, false);
  assert.equal(m.winner, 1);
  const n = newMatch();
  n.phase = "playing";
  n.scores = [49, 48];
  awardKill(n, 1, 0, false);
  assert.equal(n.phase, "playing");
  assert.equal(n.winner, null);
  n.scores = [98, 48];
  awardKill(n, 1, 0, false);
  assert.equal(n.phase, "playing");
  awardKill(n, 1, 0, false);
  assert.equal(n.scores[0], 100);
  assert.equal(n.winner, 0);
  const timed = newMatch();
  timed.phase = "playing";
  timed.time = 0.1;
  timed.scores = [2, 3];
  tickMatch(timed, 0.2);
  assert.equal(timed.winner, 1);

  const endless = newMatch();
  endless.phase = "playing";
  endless.scores = [99, 99];
  awardKill(endless, 1, 0, false, false);
  assert.deepEqual(endless.scores, [100, 99]);
  assert.equal(endless.phase, "playing");
  assert.equal(endless.winner, null);
});

test("endless team matches ignore both the score limit and match timer", () => {
  const s = game();
  try {
    s.endlessMatch = true;
    s.match.scores = [99, 99];
    s.match.time = STEP;
    const victim = s.tanks.find((tank) => tank.team === 1)!;
    const killer = s.tanks.find((tank) => tank.team === 0)!;
    victim.protection = 0;
    s.damageTank(victim, 9999, killer.id, killer.team);
    s.step();
    assert.deepEqual(s.match.scores, [100, 99]);
    assert.equal(s.match.time, STEP);
    assert.equal(s.match.phase, "playing");
    assert.equal(s.match.winner, null);
    assert.equal(s.match.overtime, false);
  } finally {
    s.dispose();
  }
});
test("complete reset restores counts, cover, pickups, scores, nav and RNG", () => {
  const s = game();
  const counts = s.snapshot().counts;
  for (const c of [...s.covers]) s.damageCover(c, 999, s.human.id, s.humanTeam);
  s.shotsFired = 100;
  s.match.scores = [20, 10];
  s.reset();
  assert.deepEqual(s.snapshot().counts, counts);
  assert.deepEqual(s.match.scores, [0, 0]);
  assert.equal(s.destroyed, 0);
  assert.ok(s.pickups.every((p) => p.available === (p.kind !== "laser")));
  s.dispose();
});
test("seeded random and team roster are reproducible and symmetric", () => {
  const a = new Random(9),
    b = new Random(9);
  assert.deepEqual(
    Array.from({ length: 20 }, () => a.next()),
    Array.from({ length: 20 }, () => b.next()),
  );
  const s = game();
  assert.equal(s.tanks.filter((t) => t.human).length, 1);
  assert.equal(s.tanks.filter((t) => t.team === 0).length, 6);
  assert.equal(s.tanks.filter((t) => t.team === 1).length, 6);
  s.dispose();
});
test("bots cross opened tower footprint and continue combat through ruins", () => {
  const s = game();
  const towers = s.covers.filter((c) => c.kind === "tower");
  for (const c of towers) s.damageCover(c, 999, s.human.id, s.humanTeam);
  // Place a scout at the entrance with a useful pickup beyond the shortcut.
  // This exercises steering through the opening without relying on a random patrol.
  const tower = towers[0];
  const scoutIndex = s.tanks.findIndex((t) => !t.human && t.kind === "scout");
  const scout = place(s, scoutIndex, tower.x, tower.z - 5);
  scout.brain.personality = "scout";
  s.pickups.push({
    id: s.nextId++,
    kind: "rapid",
    x: tower.x,
    z: tower.z + 5,
    available: true,
    cooldown: 0,
  });
  let crossed = false;
  for (let i = 0; i < 60 * 45; i++) {
    s.step(idleCommand(), true);
    for (const t of s.tanks)
      if (t.alive) {
        const p = t.body.translation();
        if (towers.some((c) => Math.abs(p.x - c.x) < 1.2 && Math.abs(p.z - c.z) < 2))
          crossed = true;
      }
  }
  assert.ok(crossed, "a bot traverses an opened shortcut");
  assert.ok(s.match.scores[0] > 0 && s.match.scores[1] > 0, JSON.stringify(s.match));
  assert.ok(s.botReroutes > 0);
  s.dispose();
});
for (const weapon of ["standard", "spread", "ricochet", "piercing", "rocket"] as const)
  test(`${weapon} stops at teammates without draining hull or shields`, () => {
    const s = game();
    clear(s);
    const shooter = place(s, 0, 0, -12);
    const ally = place(s, 2, 0, 0);
    const enemy = place(s, 1, 0, 12);
    ally.shield = 20;
    ally.shieldPoints = 120;
    const hp = ally.hp;
    shooter.aim = 0;
    shooter.selectedAmmo = weapon;
    if (weapon !== "standard") shooter.ammo[weapon] = 1;
    fireWeapon(s, shooter);
    for (const shot of s.shots) {
      shot.x = 0;
      shot.z = -5;
      shot.vx = 0;
      shot.vz = 600;
    }
    stepProjectiles(s, STEP);
    assert.equal(s.shots.length, 0);
    assert.equal(ally.hp, hp);
    assert.equal(ally.shieldPoints, 120);
    assert.equal(enemy.hp, VEHICLES[enemy.kind].health);
    assert.equal(
      s.events.some((e) => e.type === "hurt" && e.id === ally.id),
      false,
    );
    assert.ok(s.events.some((e) => e.type === "impact" && e.color === 0xb9d7e5));
    assert.equal(
      s.events.some((e) => e.type === "explosion"),
      weapon === "rocket",
    );
    s.dispose();
  });

test("bots hold fire for allies and resume when their firing lane clears", () => {
  const s = game();
  clear(s);
  const bot = place(s, 2, 0, -12);
  const ally = place(s, 0, 0, -5);
  const enemy = place(s, 1, 0, 8);
  bot.aim = 0;
  Object.assign(bot.brain, {
    target: enemy.id,
    memory: 10,
    decision: 10,
    reaction: 0,
    fireDelay: 0,
    aimError: 0,
    mode: "fight",
  });
  assert.equal(botCommand(s, bot, STEP).fire, false);
  assert.equal(bot.brain.fireDelay, 0);
  ally.body.setTranslation({ x: 12, y: 0.65, z: -5 }, true);
  assert.equal(botCommand(s, bot, STEP).fire, true);
  ally.body.setTranslation({ x: 0, y: 0.65, z: 16 }, true);
  assert.equal(friendlyBlocksShot(s, bot, 0, "standard", 35), false);
  s.dispose();
});

test("bot spread checks side pellets and ignores dead allies", () => {
  const s = game();
  clear(s);
  const bot = place(s, 2, 0, -20);
  const ally = place(s, 0, Math.sin(0.19) * 30, -20 + Math.cos(0.19) * 30);
  assert.equal(friendlyBlocksShot(s, bot, 0, "standard", 35), false);
  assert.equal(friendlyBlocksShot(s, bot, 0, "spread", 35), true);
  ally.alive = false;
  assert.equal(friendlyBlocksShot(s, bot, 0, "spread", 35), false);
  s.dispose();
});

test("breaching checks the standard shell lane even when spread ammo is preferred", () => {
  const s = game();
  try {
    clear(s);
    const bot = place(s, 2, 0, 0);
    place(s, 0, 2.6, 10);
    place(s, 1, 40, 40);
    bot.aim = 0;
    bot.ammo.spread = 5;
    bot.brain.personality = "scout";
    Object.assign(bot.brain, {
      target: 0,
      memory: 0,
      decision: 10,
      fireDelay: 0,
      goal: { x: 0, z: 13 },
    });
    s.addCover({ kind: "timber", x: 0, z: 13, w: 2, d: 1, h: 2, hp: 80, color: 0 });
    assert.equal(friendlyBlocksShot(s, bot, 0, "standard", 13), false);
    assert.equal(friendlyBlocksShot(s, bot, 0, "spread", 13), true);
    const command = botCommand(s, bot, STEP);
    assert.equal(command.ammoSelection, "standard");
    assert.equal(command.fire, true);
  } finally {
    s.dispose();
  }
});
