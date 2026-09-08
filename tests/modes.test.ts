import { test, before } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { randomArenaLayout, pickupLayout, spawnPositions } from "../src/game/arena";
import { Navigation } from "../src/game/navigation";
import { Simulation } from "../src/game/simulation";
import { collectPickup } from "../src/game/weapons";
import { botCommand } from "../src/game/ai";
import type { Cover } from "../src/game/types";
before(async () => { await RAPIER.init(); });

test("100 random maps keep pickups and spawn strips connected", () => {
  for (let seed = 1; seed <= 100; seed++) {
    const layout = randomArenaLayout(seed);
    for (const tower of layout.filter(c => c.kind === "tower"))
      assert.deepEqual([tower.w, tower.d], [6, 5], "tower supports and rubble keep their authored axes");
    assert.ok(layout.length >= 35, `cover count seed ${seed}`);
    const nav = new Navigation();
    nav.rebuild(layout.map(c => ({ ...c, alive: true })) as Cover[]);
    const points = [...pickupLayout, ...spawnPositions(0), ...spawnPositions(1)];
    const n = Math.sqrt(nav.blocked.length);
    const seen = new Set<number>([nav.index(points[0])]);
    const queue = [...seen];
    for (let i = 0; i < queue.length; i++) {
      const a = queue[i], x = a % n, z = Math.floor(a / n);
      for (const [dx, dz] of [[1,0],[-1,0],[0,1],[0,-1]]) {
        const nx = x + dx, nz = z + dz, b = nz * n + nx;
        if (nx < 0 || nx >= n || nz < 0 || nz >= n || nav.blocked[b] || seen.has(b)) continue;
        seen.add(b); queue.push(b);
      }
    }
    for (const p of points) {
      assert.equal(nav.blocked[nav.index(p)], 0, `blocked ${seed}: ${JSON.stringify(p)}`);
      assert.ok(seen.has(nav.index(p)), `unreachable ${seed}: ${JSON.stringify(p)}`);
    }
  }
  assert.deepEqual(randomArenaLayout(7), randomArenaLayout(7));
  assert.notDeepEqual(randomArenaLayout(7), randomArenaLayout(8));
});

function solo(seed = 123) {
  const s = new Simulation(seed);
  s.gameMode = "solo"; s.mapMode = "random"; s.reset(); s.start();
  for (const t of s.tanks) t.protection = 0;
  return s;
}
test("solo roster, weak armor, reduced damage, repairs, and enemy replacements", () => {
  const s = solo();
  try {
    assert.equal(s.tanks.length, 7);
    assert.equal(s.tanks.filter(t => t.team === s.humanTeam).length, 1);
    for (const t of s.tanks) assert.equal(s.nav.blocked[s.nav.index(t.body.translation())], 0);
    const enemy = s.tanks[1];
    assert.ok(enemy.hp <= 56);
    const hp = s.human.hp;
    s.damageTank(s.human, 40, enemy.id, enemy.team);
    assert.equal(s.human.hp, hp - 16);
    enemy.hp = 1;
    collectPickup(s, enemy, s.pickups.find(p => p.kind === "repair")!);
    assert.equal(enemy.hp, s.maxHealth(enemy));
    s.damageTank(enemy, 999, s.human.id, s.humanTeam);
    for (let i = 0; i < 200; i++) s.step();
    assert.equal(enemy.alive, true);
    assert.equal(s.match.phase, "playing");
  } finally { s.world.free(); }
});
test("solo survives beyond 50 kills, ends on death or ten minutes, and resets cleanly", () => {
  const s = solo();
  try {
    assert.equal(s.match.time, 600);
    const initialBodies = s.world.bodies.len();
    for (let i = 0; i < 120; i++) {
      const enemy = s.tanks.find(t => !t.human && t.alive)!;
      enemy.protection = 0;
      s.damageTank(enemy, 9999, s.human.id, s.humanTeam);
      s.reinforcementDelay = 0; s.reinforceSolo();
      assert.equal(s.match.phase, "playing", "Neither 20 nor 50 kills ends survival");
      assert.equal(s.tanks.length, 7, "Enemy slots stay bounded");
      assert.equal(enemy.xp, 0, "Replacement starts Rookie");
      assert.ok(s.world.bodies.len() <= initialBodies + s.maxFragments);
    }
    assert.equal(s.human.kills, 120);
    assert.equal(s.tanks.filter(t => !t.human && t.alive).length, 6);
    s.match.phase = "paused"; const time = s.match.time; s.step(); assert.equal(s.match.time, time);
    const previousSeed = s.mapSeed;
    s.reset(); s.start(); assert.notEqual(s.mapSeed, previousSeed);
    s.human.protection = 0;
    s.damageTank(s.human, 9999, s.tanks[1].id, s.tanks[1].team);
    assert.equal(s.match.phase, "results"); assert.notEqual(s.match.winner, s.humanTeam);
    s.reset(); s.start(); s.match.time = 0.001; s.step();
    assert.equal(s.match.phase, "results"); assert.equal(s.match.winner, s.humanTeam);
    assert.equal(s.match.time, 0); assert.equal(s.match.overtime, false);
    assert.equal(s.human.kills, 0);
    s.gameMode = "team"; s.reset();
    assert.equal(s.tanks.length, 12);
    assert.equal(s.match.time, 300);
    assert.equal(s.tanks.filter(t => t.team === 0).length, 6);
  } finally { s.world.free(); }
});
test("solo enemies fire slowly and never lay mines", () => {
  const s = solo();
  try {
    const t = s.tanks[1];
    t.brain.personality = "minelayer";
    t.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    s.human.body.setTranslation({ x: 0, y: 0.65, z: 8 }, true);
    s.world.step();
    t.brain.target = s.human.id; t.brain.memory = 3; t.brain.decision = 1;
    t.brain.reaction = 0; t.brain.aimError = 0; t.aim = 0;
    const command = botCommand(s, t, 1 / 60);
    assert.equal(command.fire, true); assert.equal(command.mine, false);
    assert.ok(t.brain.fireDelay >= 2);
  } finally { s.world.free(); }
});


test("solo reinforcements replenish six active enemies and reset the kill counter", () => {
  const s = solo();
  try {
    const active = () => s.tanks.filter(t => !t.human && t.alive);
    assert.equal(active().length, 6);
    const victim = active()[0];
    s.damageTank(victim, 999, s.human.id, s.humanTeam);
    assert.equal(active().length, 5);
    s.step();
    assert.equal(active().length, 6);
    assert.equal(victim.alive, true);
    assert.equal(s.human.kills, 1);
    for (let i = 0; i < 90; i++) {
      s.step();
      assert.ok(active().length <= 6);
    }
    s.reset();
    assert.equal(active().length, 6);
    assert.equal(s.tanks.length, 7);
    assert.equal(s.human.kills, 0);
  } finally { s.world.free(); }
});
