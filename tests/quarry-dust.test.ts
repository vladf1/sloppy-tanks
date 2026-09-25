import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { QUARRY_DUST_CAPACITY, QUARRY_DUST_MAX_OPACITY, QuarryDust } from "../src/game/quarry-dust";
import { DUST_OPACITY } from "../src/game/effect-materials";

before(async () => {
  await RAPIER.init();
});

function fixture(map: "village" | "quarry" = "quarry") {
  const sim = new Simulation(123);
  sim.mapMode = map;
  sim.reset(2);
  sim.start();
  const dust = new QuarryDust();
  const step = (dt = 1 / 60) => {
    sim.elapsed += dt;
    dust.update(sim, dt);
  };
  return { sim, dust, step };
}

test("wind wisps stay within the fixed pool, opacity cap and finite poses", () => {
  const { dust, step } = fixture();
  try {
    for (let i = 0; i < 2000; i++) step(1 / 30);
    assert.ok(dust.mesh.count > 0, "sparse wisps accumulate while playing");
    assert.ok(dust.mesh.count <= QUARRY_DUST_CAPACITY);
    const opacity = dust.mesh.geometry.getAttribute(DUST_OPACITY);
    const matrices = dust.mesh.instanceMatrix.array;
    for (let i = 0; i < dust.mesh.count; i++) {
      assert.ok(opacity.getX(i) <= QUARRY_DUST_MAX_OPACITY + 1e-6);
      for (let k = 0; k < 16; k++) {
        assert.ok(Number.isFinite(matrices[i * 16 + k]));
      }
    }
  } finally {
    dust.reset();
  }
});

test("dust freezes while paused, hides off-quarry and clears on reset", () => {
  const { sim, dust, step } = fixture();
  try {
    for (let i = 0; i < 60; i++) step();
    assert.ok(dust.mesh.count > 0);
    assert.equal(dust.mesh.visible, true);
    const matrices = dust.mesh.instanceMatrix.array.slice();
    sim.match.phase = "paused";
    for (let i = 0; i < 30; i++) step();
    assert.deepEqual(dust.mesh.instanceMatrix.array, matrices);
    sim.match.phase = "playing";
    dust.reset();
    assert.equal(dust.mesh.count, 0);
    assert.equal(dust.mesh.visible, false);
    step();
    assert.equal(dust.mesh.visible, true);
  } finally {
    dust.reset();
  }
});

test("switching away from the quarry stops and hides the effect", () => {
  const quarry = fixture("quarry");
  const village = fixture("village");
  try {
    for (let i = 0; i < 60; i++) quarry.step();
    assert.ok(quarry.dust.mesh.count > 0);
    for (let i = 0; i < 60; i++) village.step();
    assert.equal(village.dust.mesh.count, 0);
    assert.equal(village.dust.mesh.visible, false);
    // A stale quarry pool clears itself the moment the theme changes.
    quarry.sim.mapMode = "village";
    quarry.sim.reset(2);
    quarry.sim.start();
    quarry.step();
    assert.equal(quarry.dust.mesh.count, 0);
    assert.equal(quarry.dust.mesh.visible, false);
  } finally {
    quarry.dust.reset();
    village.dust.reset();
    quarry.sim.dispose();
    village.sim.dispose();
  }
});
