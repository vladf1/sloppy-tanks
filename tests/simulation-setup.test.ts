import assert from "node:assert/strict";
import { before, mock, test } from "node:test";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation, type SimulationSetup } from "../src/game/simulation";
import { idleCommand } from "../src/game/types";
import { STEP } from "../src/game/data";

before(async () => {
  await RAPIER.init();
});

function snapshot(sim: Simulation) {
  return {
    match: sim.match,
    map: sim.mapName,
    rng: sim.rng.state,
    nextId: sim.nextId,
    pickups: sim.pickups,
    bodies: sim.world.bodies.len(),
    colliders: sim.world.colliders.len(),
    tanks: sim.tanks.map(({ body, collider: _collider, ...tank }) => ({
      ...tank,
      position: body.translation(),
      velocity: body.linvel(),
    })),
    covers: sim.covers.map(({ id, kind, x, z, hp, alive }) => ({ id, kind, x, z, hp, alive })),
    shots: sim.shots,
  };
}

test("direct setup builds one world and preserves legacy seeded rounds and subsequent resets", () => {
  // One team and one solo round cover the options that change roster and RNG order.
  const cases: SimulationSetup[] = [
    { mapMode: "harbor", humanTeam: 0, humanKind: "scout" },
    { mapMode: "quarry", gameMode: "solo", difficulty: "hard" },
  ];
  for (const options of cases) {
    const reset = mock.method(Simulation.prototype, "reset");
    const direct = new Simulation(20402, { ...options, round: 3 });
    assert.equal(reset.mock.callCount(), 1, "only the requested world is built");
    reset.mock.restore();
    const legacy = new Simulation(20402);
    try {
      Object.assign(legacy, options);
      legacy.reset();
      assert.deepEqual(snapshot(direct), snapshot(legacy));
      direct.start();
      legacy.start();
      for (let i = 0; i < Math.round(2 / STEP); i++) {
        direct.step(idleCommand(), true);
        legacy.step(idleCommand(), true);
      }
      assert.deepEqual(snapshot(direct), snapshot(legacy), "seeded combat must remain identical");
      direct.reset();
      legacy.reset();
      assert.deepEqual(snapshot(direct), snapshot(legacy), "later rounds retain map/RNG order");
    } finally {
      direct.dispose();
      legacy.dispose();
    }
  }
});
