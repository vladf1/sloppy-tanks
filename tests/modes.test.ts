import { test, before } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { pickupLayout, spawnPositions, type CoverDef } from "../src/game/arena";
import { MAPS } from "../src/game/maps";
import { Navigation } from "../src/game/navigation";
import { Simulation } from "../src/game/simulation";
import { collectPickup } from "../src/game/weapons";
import { botCommand } from "../src/game/ai";
import type { Cover } from "../src/game/types";
before(async () => {
  await RAPIER.init();
});

test("authored maps keep every pickup and spawn connected", () => {
  for (const map of MAPS) {
    const nav = new Navigation();
    nav.rebuild(map.layout().map((c) => ({ ...c, alive: true })) as Cover[]);
    for (const p of [...pickupLayout, ...spawnPositions(0), ...spawnPositions(1)]) {
      assert.equal(nav.blocked[nav.index(p)], 0, `blocked ${map.id}: ${JSON.stringify(p)}`);
      assert.ok(
        nav.find({ x: 0, z: 0 }, p).length > 0 || (p.x === 0 && p.z === 0),
        `unreachable ${map.id}: ${JSON.stringify(p)}`,
      );
    }
  }
});

function solo(seed = 123) {
  const s = new Simulation(seed);
  s.gameMode = "solo";
  s.mapMode = "surprise";
  s.reset();
  s.start();
  for (const t of s.tanks) t.protection = 0;
  return s;
}
test("solo roster, weak armor, reduced damage, repairs, and enemy replacements", () => {
  const s = solo();
  try {
    assert.equal(s.tanks.length, 7);
    assert.equal(s.tanks.filter((t) => t.team === s.humanTeam).length, 1);
    for (const t of s.tanks) assert.equal(s.nav.blocked[s.nav.index(t.body.translation())], 0);
    const enemy = s.tanks[1];
    assert.ok(enemy.hp <= 56);
    const hp = s.human.hp;
    s.damageTank(s.human, 40, enemy.id, enemy.team);
    assert.equal(s.human.hp, hp - 16);
    enemy.hp = 1;
    collectPickup(
      s,
      enemy,
      s.pickups.find((p) => p.kind === "repair")!,
    );
    assert.equal(enemy.hp, s.maxHealth(enemy));
    s.damageTank(enemy, 999, s.human.id, s.humanTeam);
    for (let i = 0; i < 200; i++) s.step();
    assert.equal(enemy.alive, true);
    assert.equal(s.match.phase, "playing");
  } finally {
    s.world.free();
  }
});
test("solo survives beyond 50 kills, ends on death or ten minutes, and resets cleanly", () => {
  const s = solo();
  try {
    assert.equal(s.match.time, 600);
    const initialBodies = s.world.bodies.len();
    for (let i = 0; i < 120; i++) {
      const enemy = s.tanks.find((t) => !t.human && t.alive)!;
      enemy.protection = 0;
      s.damageTank(enemy, 9999, s.human.id, s.humanTeam);
      s.reinforcementDelay = 0;
      s.reinforceSolo();
      assert.equal(s.match.phase, "playing", "Neither 20 nor 50 kills ends survival");
      assert.equal(s.tanks.length, 7, "Enemy slots stay bounded");
      assert.equal(enemy.xp, 0, "Replacement starts Rookie");
      assert.ok(s.world.bodies.len() <= initialBodies + s.maxFragments);
    }
    assert.equal(s.human.kills, 120);
    assert.equal(s.tanks.filter((t) => !t.human && t.alive).length, 6);
    s.match.phase = "paused";
    const time = s.match.time;
    s.step();
    assert.equal(s.match.time, time);
    s.reset();
    s.start();
    s.human.protection = 0;
    s.damageTank(s.human, 9999, s.tanks[1].id, s.tanks[1].team);
    assert.equal(s.match.phase, "results");
    assert.notEqual(s.match.winner, s.humanTeam);
    s.reset();
    s.start();
    s.match.time = 0.001;
    s.step();
    assert.equal(s.match.phase, "results");
    assert.equal(s.match.winner, s.humanTeam);
    assert.equal(s.match.time, 0);
    assert.equal(s.match.overtime, false);
    assert.equal(s.human.kills, 0);
    s.gameMode = "team";
    s.reset();
    assert.equal(s.tanks.length, 12);
    assert.equal(s.match.time, 300);
    assert.equal(s.tanks.filter((t) => t.team === 0).length, 6);
  } finally {
    s.world.free();
  }
});
test("solo enemies fire slowly and never lay mines", () => {
  const s = solo();
  try {
    const t = s.tanks[1];
    t.brain.personality = "minelayer";
    t.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    s.human.body.setTranslation({ x: 0, y: 0.65, z: 8 }, true);
    s.world.step();
    t.brain.target = s.human.id;
    t.brain.memory = 3;
    t.brain.decision = 1;
    t.brain.reaction = 0;
    t.brain.aimError = 0;
    t.aim = 0;
    const command = botCommand(s, t, 1 / 60);
    assert.equal(command.fire, true);
    assert.equal(command.mine, false);
    assert.ok(t.brain.fireDelay >= 2);
  } finally {
    s.world.free();
  }
});

test("solo reinforcements replenish six active enemies and reset the kill counter", () => {
  const s = solo();
  try {
    const active = () => s.tanks.filter((t) => !t.human && t.alive);
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
  } finally {
    s.world.free();
  }
});

test("Surprise me picks complete authored maps in both modes and keeps the choice for the match", () => {
  const signature = (covers: CoverDef[]) =>
    covers.map(({ kind, x, z, w, d, h, hp, color }) => [kind, x, z, w, d, h, hp, color]);
  for (const gameMode of ["team", "solo"] as const) {
    const sim = new Simulation(912);
    const replay = new Simulation(912);
    try {
      sim.gameMode = replay.gameMode = gameMode;
      sim.mapMode = replay.mapMode = "surprise";
      const selected = new Set<string>();
      for (let round = 0; round < 20; round++) {
        sim.reset();
        replay.reset();
        const map = MAPS.find((map) => map.id === sim.mapTheme)!;
        selected.add(map.id);
        assert.equal(sim.mapMode, "surprise", "selection survives a new round");
        assert.equal(sim.mapTheme, replay.mapTheme, "seeded matches remain reproducible");
        assert.equal(sim.mapName, map.name.toUpperCase(), "show the actual battlefield name");
        assert.deepEqual(
          signature(sim.covers),
          signature(map.layout()),
          "use the entire authored layout",
        );
        assert.equal(sim.tanks.length, gameMode === "solo" ? 7 : 12);
        sim.start();
        sim.step();
        sim.match.phase = "paused";
        sim.step();
        assert.equal(sim.mapTheme, map.id, "playing and pausing never reroll the map");
      }
      assert.deepEqual(selected, new Set(MAPS.map((map) => map.id)));
      for (const map of MAPS) {
        sim.mapMode = map.id;
        sim.reset();
        assert.equal(sim.mapTheme, map.id, "manual selection overrides Surprise me");
        assert.deepEqual(signature(sim.covers), signature(map.layout()));
      }
    } finally {
      sim.world.free();
      replay.world.free();
    }
  }
});
