import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { createMultiplayerSimulation, setDriver } from "../src/net/multiplayer-simulation";
import { Simulation } from "../src/game/simulation";
import { idleCommand, type PlayerAssignment } from "../src/game/types";
import { STEP, VEHICLES, WEAPONS } from "../src/game/data";
import { enemyDifficulty } from "../src/game/difficulty";
import { newCombatRecord } from "../src/game/combat-record";
import { cleanupCandidate } from "../src/game/debris-cleanup";
import { tuneSpeed } from "../src/game/speed-tuning";
import { fireWeapon, placeMine } from "../src/game/weapons";

before(async () => {
  await RAPIER.init();
});
const players: PlayerAssignment[] = [
  { playerId: "alice", name: "Alice", team: 0, slot: 0, kind: "scout" },
  { playerId: "bob", name: "Bob", team: 1, slot: 0, kind: "heavy" },
];
for (const mapMode of ["village", "harbor", "quarry"] as const) {
  test(`${mapMode}: humans-only creates just assigned seats, including sparse slots`, () => {
    const roster = players.map((player, index) => ({ ...player, slot: index + 3 }));
    const sim = createMultiplayerSimulation(4242, roster, { mapMode, humansOnly: true });
    try {
      assert.equal(sim.tanks.length, 2);
      assert.ok(sim.tanks.every((tank) => tank.human && tank.driver === "human"));
      assert.deepEqual(
        sim.tanks.map((tank) => tank.playerId),
        ["alice", "bob"],
      );
      sim.start();
      for (let tick = 0; tick < 180; tick++) sim.stepWith(new Map());
      assert.equal(sim.shotsFired, 0);
      sim.reset();
      assert.equal(sim.tanks.length, 2);
    } finally {
      sim.dispose();
    }
  });
}
function arena(sim: Simulation) {
  for (const cover of sim.covers) if (cover.body.isValid()) sim.world.removeRigidBody(cover.body);
  for (const tank of sim.tanks.filter((tank) => !tank.human)) sim.world.removeRigidBody(tank.body);
  sim.covers = [];
  sim.movableCovers = [];
  sim.coverByCollider.clear();
  sim.nav.rebuild([]);
  sim.tanks = sim.tanks.filter((tank) => tank.human);
  sim.pickups = [];
  sim.tanks.forEach((tank, i) => {
    tank.body.setTranslation({ x: i * 20 - 10, y: 0.65, z: 0 }, true);
    tank.heading = 0;
    tank.protection = 0;
  });
  sim.world.step();
  sim.start();
}

test("multiplayer fills twelve stable slots, validates ownership and enforces player capacities", () => {
  const roster = Array.from({ length: 8 }, (_, i): PlayerAssignment => ({
    ...players[i % 2],
    playerId: `p${i}`,
    slot: Math.floor(i / 2),
  }));
  const sim = createMultiplayerSimulation(4242, roster);
  try {
    assert.equal(sim.tanks.length, 12);
    assert.equal(sim.tanks.filter((tank) => tank.human).length, 8);
    assert.equal(sim.tanks.filter((tank) => tank.team === 0).length, 6);
    assert.equal(sim.tanks.find((tank) => tank.playerId === "p1")!.kind, "heavy");
    const ids = sim.tanks.map((tank) => tank.id);
    sim.reset();
    assert.deepEqual(
      sim.tanks.map((tank) => tank.id),
      ids,
    );
    assert.equal(sim.tanks.filter((tank) => tank.human).length, 8);
    assert.throws(() =>
      setDriver(
        sim.tanks.find((tank) => !tank.human)!,
        "human",
      ),
    );
  } finally {
    sim.dispose();
  }
  for (const invalid of [
    [...roster, { ...players[0], playerId: "ninth", slot: 5 }],
    [players[0], players[0]],
    [{ ...players[0], slot: 6 }],
    [{ ...players[0], kind: "humvee" }],
    [{ ...players[0], name: " " }],
  ])
    assert.throws(() => createMultiplayerSimulation(4242, invalid as PlayerAssignment[]));
});

test("two player commands move independently; omitted commands idle and actions are one tick", () => {
  const sim = createMultiplayerSimulation(4242, players);
  try {
    arena(sim);
    const [a, b] = sim.tanks;
    for (let i = 0; i < 90; i++)
      sim.stepWith(
        new Map([
          [a.id, { ...idleCommand(), moveZ: 1, aim: 0.3 }],
          [b.id, { ...idleCommand(), moveZ: -1, aim: -0.4 }],
        ]),
      );
    assert.ok(a.body.translation().z > 5);
    assert.ok(b.body.translation().z < -3);
    assert.equal(a.aim, 0.3);
    assert.equal(b.aim, -0.4);
    a.ammo.rocket = 3;
    sim.stepWith(new Map([[a.id, { ...idleCommand(), mine: true, ammoSelection: "rocket" }]]));
    assert.equal(sim.mines.length, 1);
    assert.equal(a.selectedAmmo, "rocket");
    for (let i = 0; i < 400; i++) sim.stepWith(new Map());
    assert.equal(sim.mines.length, 1, "a missing command never repeats the mine action");
    assert.equal(a.command.fire, false);
    assert.equal(a.command.mine, false);
    assert.equal(a.command.aim, 0, "last explicit aim survives missing input");
    assert.ok(Math.hypot(a.body.linvel().x, a.body.linvel().z) < 0.01);
  } finally {
    sim.dispose();
  }
});

test("driver handoff preserves player balance and respawn uses each seat's chassis", () => {
  const sim = createMultiplayerSimulation(4242, players, { difficulty: "hard" });
  try {
    const [a, b] = sim.tanks.filter((tank) => tank.human);
    a.kills = 4;
    a.xp = 100;
    const identity = [a.id, a.life, a.kind, a.team, a.kills, a.xp, sim.maxHealth(a)];
    setDriver(a, "bot");
    assert.deepEqual([a.id, a.life, a.kind, a.team, a.kills, a.xp, sim.maxHealth(a)], identity);
    assert.equal(enemyDifficulty(sim, a).damage, 1);
    for (const bot of sim.tanks.filter((tank) => !tank.human))
      assert.equal(enemyDifficulty(sim, bot).damage, 1.15);
    sim.start();
    sim.stepWith(new Map());
    assert.equal(a.driver, "bot");
    setDriver(a, "human");
    a.protection = b.protection = 0;
    sim.damageTank(a, 10000, b.id, b.team, b.life);
    sim.damageTank(b, 10000, a.id, a.team, a.life);
    for (let i = 0; i < 240; i++) sim.stepWith(new Map());
    assert.equal(a.kind, "scout");
    assert.equal(b.kind, "heavy");
    assert.equal(a.alive, true);
    assert.equal(b.alive, true);
    assert.equal(a.life, 1);
    assert.equal(a.deaths, 1);
    assert.equal(b.life, 1);
    assert.equal(b.deaths, 1);
    assert.equal(tuneSpeed(sim, "tank-speed", 2), 1);
    assert.equal(tuneSpeed(sim, "bullet-speed", 0.5), 1);
  } finally {
    sim.dispose();
  }
});

test("old-life ordnance cannot award replacement XP or life kills; multiplayer recap stays empty", () => {
  const sim = createMultiplayerSimulation(4242, players);
  try {
    arena(sim);
    const [a, b] = sim.tanks;
    const oldLife = a.life;
    fireWeapon(sim, a);
    placeMine(sim, a);
    assert.equal(sim.shots[0].ownerLife, oldLife);
    assert.equal(sim.mines[0].ownerLife, oldLife);
    a.life++; // A future seat reassignment starts a new generation without a fake death.
    sim.damageTank(b, 10000, a.id, a.team, oldLife);
    assert.equal(a.deaths, 0);
    assert.equal(a.kills, 1);
    assert.equal(a.lifeKills, 0);
    assert.equal(a.xp, 0);
    assert.deepEqual(sim.combatRecord, newCombatRecord());
    assert.ok(
      sim.events
        .filter((event) => event.type === "death")
        .every((event) => event.label === undefined),
    );
    const drum = sim.addCover({
      kind: "drum",
      x: 40,
      z: 40,
      w: 1,
      h: 2,
      d: 1,
      hp: 20,
      color: 0x888888,
    });
    sim.damageCover(drum, 100, a.id, a.team, a.life);
    for (const event of sim.events) {
      assert.equal("body" in event, false);
      assert.equal("collider" in event, false);
      assert.deepEqual(JSON.parse(JSON.stringify(event)).x, event.x);
    }
  } finally {
    sim.dispose();
  }
});

test("fill-bot damage applies equally on both teams and never changes player-seat damage", () => {
  const sim = createMultiplayerSimulation(4242, players, { difficulty: "hard" });
  try {
    for (const victim of sim.tanks.filter((t) => t.human)) {
      const bot = sim.tanks.find((t) => !t.human && t.team !== victim.team)!;
      victim.protection = 0;
      const hp = victim.hp;
      sim.damageTank(victim, 10, bot.id, bot.team, bot.life);
      assert.equal(victim.hp, hp - 11.5);
    }
    const [a, b] = sim.tanks.filter((t) => t.human);
    setDriver(a, "bot");
    const hp = b.hp;
    sim.damageTank(b, 10, a.id, a.team, a.life);
    assert.equal(b.hp, hp - 10);
  } finally {
    sim.dispose();
  }
});

test("debris cleanup preserves proximity to either player rather than just the first viewer", () => {
  const sim = createMultiplayerSimulation(4242, players);
  try {
    arena(sim);
    sim.tanks[0].body.setTranslation({ x: -50, y: 0.65, z: 0 }, true);
    sim.tanks[1].body.setTranslation({ x: 50, y: 0.65, z: 0 }, true);
    sim.fragment(50, 0, 0x888888, 1);
    sim.fragment(0, 0, 0x888888, 1);
    for (const f of sim.fragments) {
      f.body.sleep();
      f.life = 5;
    }
    assert.equal(cleanupCandidate(sim)?.id, sim.fragments[1].id);
  } finally {
    sim.dispose();
  }
});

function runTuned(sim: Simulation, ticks: number) {
  for (let i = 0; i < ticks; i++) sim.step({ ...idleCommand(), moveZ: 1, aim: 0, fire: true });
  return {
    state: sim.snapshot(),
    shots: sim.shots.map((s) => ({ x: s.x, z: s.z, vx: s.vx, vz: s.vz })),
    rng: sim.rng.state,
  };
}
test("interleaved worlds keep speed tuning isolated, including new bodies and existing projectiles", () => {
  const baseSpeed = VEHICLES.balanced.speed,
    shellSpeed = WEAPONS.standard.speed;
  const sims = Array.from({ length: 4 }, () => new Simulation(4242));
  try {
    for (const sim of sims) sim.start();
    for (const sim of [sims[0], sims[2]]) {
      tuneSpeed(sim, "tank-speed", 1.5);
      tuneSpeed(sim, "bullet-speed", 0.5);
    }
    for (const sim of [sims[1], sims[3]]) {
      tuneSpeed(sim, "tank-speed", 0.7);
      tuneSpeed(sim, "bullet-speed", 1.8);
    }
    const a = runTuned(sims[0], 180),
      b = runTuned(sims[1], 180);
    for (let i = 0; i < 180; i++) {
      runTuned(sims[2], 1);
      runTuned(sims[3], 1);
    }
    assert.deepEqual(runTuned(sims[2], 0), a);
    assert.deepEqual(runTuned(sims[3], 0), b);
    assert.equal(VEHICLES.balanced.speed, baseSpeed);
    assert.equal(WEAPONS.standard.speed, shellSpeed);
    sims[0].reset();
    assert.ok(
      Math.abs(sims[0].human.body.softCcdPrediction() - baseSpeed * 1.5 * 1.5 * STEP * 2) < 1e-6,
    );
    const fresh = new Simulation(4242);
    try {
      assert.deepEqual(fresh.speedTuning, { "tank-speed": 1, "bullet-speed": 1 });
    } finally {
      fresh.dispose();
    }
  } finally {
    sims.forEach((s) => s.dispose());
  }
});

test("viewer-specific rendering cannot affect multiplayer wreck placement or simulation", async () => {
  const { renderState } = await import("../src/game/render-state");
  const a = createMultiplayerSimulation(4242, players),
    b = createMultiplayerSimulation(4242, players);
  try {
    const humans = a.tanks.filter((t) => t.human);
    const first = renderState(a, { minX: -1, maxX: 1, minZ: -1, maxZ: 1 }, humans[0].id);
    const second = renderState(a, { minX: 90, maxX: 100, minZ: 90, maxZ: 100 }, humans[1].id);
    assert.notEqual(first, second);
    assert.equal("playerId" in first.viewer, false); // Ownership stays outside presentation.
    assert.equal(first.viewer.id, humans[0].id);
    assert.equal(second.viewer.id, humans[1].id);
    assert.equal(renderState(a, undefined, humans[0].id), first);
    assert.equal(a.wreckView, undefined);
    assert.throws(() => renderState(a, undefined, -123));
    for (const sim of [a, b]) {
      sim.start();
      const victim = sim.tanks.find((t) => t.human)!;
      victim.protection = 0;
      sim.damageTank(victim, 10000, -1, 1);
      for (let i = 0; i < 60; i++) sim.stepWith(new Map());
    }
    assert.deepEqual(a.snapshot(), b.snapshot());
    assert.deepEqual(
      a.fragments.map((f) => ({ ...f.body.translation() })),
      b.fragments.map((f) => ({ ...f.body.translation() })),
    );
    assert.equal(a.rng.state, b.rng.state);
  } finally {
    a.dispose();
    b.dispose();
  }
});

for (const mapMode of ["village", "harbor", "quarry"] as const) {
  for (const difficulty of ["easy", "normal", "hard"] as const) {
    for (const gameMode of ["team", "solo"] as const) {
      test(`${mapMode}/${difficulty}/${gameMode}: per-tank dispatch preserves legacy command and RNG order`, () => {
        const before = new Simulation(4242, { mapMode, difficulty, gameMode });
        const after = new Simulation(4242, { mapMode, difficulty, gameMode });
        try {
          before.start();
          after.start();
          for (let i = 0; i < 90; i++) {
            const command = {
              ...idleCommand(),
              moveZ: i < 30 ? 1 : 0,
              moveX: i >= 30 ? 1 : 0,
              aim: i / 90,
              fire: true,
              mine: i === 20,
            };
            before.step(command);
            after.stepWith(new Map([[after.human.id, command]]));
          }
          assert.deepEqual(after.snapshot(), before.snapshot());
          assert.equal(after.rng.state, before.rng.state);
          assert.deepEqual(after.combatRecord, before.combatRecord);
        } finally {
          before.dispose();
          after.dispose();
        }
      });
    }
  }
}
