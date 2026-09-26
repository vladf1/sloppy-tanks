import assert from "node:assert/strict";
import { before, test } from "node:test";
import RAPIER from "@dimforge/rapier3d-compat";
import {
  STRESS_PLAYER_HEALTH_MULTIPLIER,
  STRESS_PLAYER_KIND,
  STRESS_TANK_COUNT,
  STRESS_TEST_MAP,
  STRESS_TEST_SETUP,
} from "../src/stress-test-level";
import { MAPS } from "../src/game/maps";
import { Simulation } from "../src/game/simulation";
import { STEP, VEHICLES } from "../src/game/data";

before(async () => {
  await RAPIER.init();
});

test("stress pickups and all 30 spawns have hull clearance and navigable routes", () => {
  const sim = new Simulation(731, { ...STRESS_TEST_SETUP, round: 3 });
  try {
    assert.equal(sim.tanks.length, STRESS_TANK_COUNT);
    const points = [...sim.pickups, ...sim.tanks.map((tank) => tank.body.translation())];
    for (const point of points) {
      assert.equal(sim.nav.blocked[sim.nav.index(point)], 0, `blocked ${JSON.stringify(point)}`);
      assert.ok(
        (point.x === 0 && point.z === 0) || sim.nav.find({ x: 0, z: 0 }, point).length > 0,
        `unreachable ${JSON.stringify(point)}`,
      );
      for (const cover of sim.covers) {
        assert.ok(
          Math.abs(point.x - cover.x) >= cover.w / 2 + 1.5 ||
            Math.abs(point.z - cover.z) >= cover.d / 2 + 1.5,
          `no hull clearance at ${JSON.stringify(point)} beside ${cover.kind}`,
        );
      }
    }
  } finally {
    sim.dispose();
  }
});

test("stress grid is a bounded, dense destruction workload", () => {
  const layout = STRESS_TEST_MAP.layout();
  const destructible = layout.filter((cover) => Number.isFinite(cover.hp));
  const permanentHouses = layout.filter(
    (cover) => cover.kind === "house" && !Number.isFinite(cover.hp),
  );
  const dragonTeeth = layout.filter((cover) => cover.kind === "teeth");
  const drums = layout.filter((cover) => cover.kind === "drum");
  const keys = layout.map((cover) => `${cover.kind}:${cover.x}:${cover.z}`);

  assert.equal(layout.filter((cover) => cover.kind === "boundary").length, 4);
  assert.equal(destructible.length, 75);
  assert.equal(permanentHouses.length, 8);
  assert.ok(dragonTeeth.length >= 24);
  assert.ok(dragonTeeth.every((cover) => !Number.isFinite(cover.hp)));
  assert.ok(drums.length > 0);
  assert.ok(drums.every((cover) => Math.hypot(cover.x, cover.z) > 15));
  assert.ok(layout.filter((cover) => cover.kind === "tree").length >= 20);
  assert.ok(layout.some((cover) => cover.kind === "hedgehog"));
  assert.equal(
    new Set(keys).size,
    keys.length,
    "every stress obstacle needs a unique location and kind",
  );
  assert.equal(STRESS_TEST_MAP.floor, "dry-grass");
  assert.equal(STRESS_TEST_MAP.outerFloor, "packed-dirt");
  assert.equal(STRESS_TEST_MAP.outerFloorExtent, 140);
  assert.equal(STRESS_TEST_MAP.theme, undefined);
});

test("stress objects keep the authored maps' destructibility rules", () => {
  const authored = MAPS.flatMap((map) => map.layout());
  const alwaysPermanent = new Set(
    authored
      .filter(
        (cover) =>
          !Number.isFinite(cover.hp) &&
          !authored.some(
            (candidate) => candidate.kind === cover.kind && Number.isFinite(candidate.hp),
          ),
      )
      .map((cover) => cover.kind),
  );

  for (const cover of STRESS_TEST_MAP.layout()) {
    if (alwaysPermanent.has(cover.kind)) {
      assert.equal(cover.hp, Infinity, `${cover.kind} cannot become destructible in stress mode`);
    }
  }
});

test("stress configuration survives respawns and resets and never ends at the normal limits", () => {
  const sim = new Simulation(731, { ...STRESS_TEST_SETUP, round: 3 });
  try {
    const initialBodies = sim.world.bodies.len();
    for (let round = 0; round < 2; round++) {
      assert.equal(sim.mapName, "STRESS GRID");
      assert.equal(sim.tanks.filter((tank) => tank.team === 0).length, 15);
      assert.equal(sim.tanks.filter((tank) => tank.team === 1).length, 15);
      assert.equal(sim.human.kind, STRESS_PLAYER_KIND);
      const hp = VEHICLES[STRESS_PLAYER_KIND].health * STRESS_PLAYER_HEALTH_MULTIPLIER;
      assert.equal(sim.human.hp, hp);
      sim.start();
      sim.match.scores = [100, 100];
      sim.match.time = STEP;
      const enemy = sim.tanks.find((tank) => tank.team !== sim.human.team)!;
      enemy.protection = 0;
      sim.damageTank(enemy, 9999, sim.human.id, sim.human.team);
      sim.step();
      assert.equal(sim.match.scores[sim.human.team], 101);
      assert.equal(sim.match.phase, "playing");
      assert.equal(sim.match.winner, null);
      sim.human.protection = 0;
      sim.damageTank(sim.human, sim.maxHealth(sim.human) * 2, enemy.id, enemy.team);
      sim.respawn(sim.human);
      assert.equal(sim.human.hp, hp);
      sim.reset();
      assert.equal(sim.world.bodies.len(), initialBodies);
      assert.equal(sim.fragments.length, 0);
      assert.deepEqual(sim.match.scores, [0, 0]);
    }
  } finally {
    sim.dispose();
  }
});
