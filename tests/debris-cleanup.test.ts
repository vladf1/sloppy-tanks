import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { STEP } from "../src/game/data";
import { cleanupCandidate, prepareDebrisCleanup } from "../src/game/debris-cleanup";

before(async () => {
  await RAPIER.init();
});
function arena() {
  const sim = new Simulation();
  for (const tank of sim.tanks) sim.world.removeRigidBody(tank.body);
  sim.tanks = [];
  sim.start();
  return sim;
}
function fragment(sim: Simulation, x: number, moving: boolean) {
  sim.fragment(x, 0, 0x805336);
  const f = sim.fragments.at(-1)!;
  f.life = 2;
  if (!moving) f.body.sleep();
  return f;
}

test("budget eviction preserves nearby moving debris ahead of distant settled pieces", () => {
  const sim = arena();
  try {
    const nearby = fragment(sim, 0, true);
    const distant = fragment(sim, 40, false);
    assert.equal(cleanupCandidate(sim), distant);
    sim.maxFragments = 2;
    fragment(sim, 2, true);
    assert.ok(sim.fragments.includes(nearby));
    assert.ok(!sim.fragments.includes(distant));
    assert.equal(distant.body.isValid(), false);
    assert.equal(sim.fragments.length, 2);
  } finally {
    sim.dispose();
  }
});

test("pressure starts a gradual cleanup of old settled pieces without deleting them", () => {
  const sim = arena();
  try {
    sim.maxFragments = 5;
    const settled = fragment(sim, 40, false);
    for (let i = 0; i < 4; i++) fragment(sim, i, true);
    prepareDebrisCleanup(sim);
    assert.equal(settled.life, 1);
    assert.equal(sim.fragments.length, 5);
    assert.ok(sim.fragments.filter((f) => f !== settled).every((f) => f.life === 2));
    prepareDebrisCleanup(sim);
    assert.equal(sim.fragments.filter((f) => f.life === 1).length, 1);
  } finally {
    sim.dispose();
  }
});

test("moving substantial debris delays cleanup but still obeys its hard expiration", () => {
  const sim = arena();
  try {
    const f = fragment(sim, 0, true);
    f.body.setTranslation({ x: 0, y: 10, z: 0 }, true);
    f.body.setLinvel({ x: 3, y: 0, z: 0 }, true);
    f.life = 1 + STEP / 2;
    f.expiresAt = 18;
    sim.step();
    assert.ok(f.life > 1, "moving piece has not started sinking");
    f.expiresAt = sim.elapsed + 1;
    sim.step();
    assert.ok(f.life < 1, "hard expiration starts cleanup even while moving");
    for (let i = 0; i < 61; i++) sim.step();
    assert.ok(!sim.fragments.includes(f));
  } finally {
    sim.dispose();
  }
});
