import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Matrix4, Vector3 } from "three";
import { Simulation } from "../src/game/simulation";
import { TRACK_GRAVEL_CAPACITY } from "../src/game/track-gravel";
import { STRESS_TEST_MAP } from "../src/stress-test-level";
import { TrackTrails } from "../src/game/tracks";
import { TrackDust, TRACK_DUST_CAPACITY } from "../src/game/track-dust";

before(async () => {
  await RAPIER.init();
});

function fixture(count = 1, map: "village" | "quarry" | "harbor" = "village", customGrass = false) {
  const sim = new Simulation(123);
  sim.mapMode = map;
  if (customGrass) sim.customMap = STRESS_TEST_MAP;
  sim.reset(2);
  for (const tank of sim.tanks) sim.world.removeRigidBody(tank.body);
  sim.tanks = [];
  for (let i = 0; i < count; i++) {
    const tank = sim.addTank(0, i === 0, "balanced");
    tank.heading = 0;
    tank.body.setTranslation({ x: i * 5, y: 0.65, z: 0 }, true);
  }
  sim.start();
  const dust = new TrackDust();
  const step = (speed: number, dt = 1 / 60, height = 0.65) => {
    sim.elapsed += dt;
    for (const tank of sim.tanks) {
      const p = tank.body.translation();
      tank.body.setTranslation({ x: p.x, y: height, z: p.z + speed * dt }, true);
      tank.body.setLinvel({ x: 0, y: 0, z: speed }, true);
    }
    dust.update(sim);
  };
  dust.update(sim);
  step(0);
  return { sim, dust, step };
}

test("track dust comes from both trailing tracks in forward and reverse", () => {
  for (const speed of [12, -12]) {
    const { sim, dust, step } = fixture();
    for (let i = 0; i < 15; i++) step(speed);
    assert.ok(dust.mesh.count >= 2);
    const matrix = new Matrix4();
    const points = [0, 1].map((i) => {
      dust.mesh.getMatrixAt(i, matrix);
      return new Vector3().setFromMatrixPosition(matrix);
    });
    assert.ok(points[0].x < 0 && points[1].x > 0);
    for (const p of points) assert.ok((p.z - sim.human.body.translation().z) * speed < 0);
    sim.dispose();
  }
});

test("dust freezes without simulation ticks, expires after stopping and clears on reset", () => {
  const { sim, dust, step } = fixture();
  for (let i = 0; i < 60; i++) step(12);
  const count = dust.mesh.count;
  const matrices = dust.mesh.instanceMatrix.array.slice();
  for (let i = 0; i < 120; i++) dust.update(sim);
  assert.equal(dust.mesh.count, count);
  assert.deepEqual(dust.mesh.instanceMatrix.array, matrices);
  for (let i = 0; i < 70; i++) step(0);
  assert.equal(dust.mesh.count, 0);
  for (let i = 0; i < 30; i++) step(12);
  assert.ok(dust.mesh.count > 0);
  dust.reset();
  assert.equal(dust.mesh.count, 0);
  sim.dispose();
});

test("idle, airborne, dead tanks and teleports do not raise dust", () => {
  const { sim, dust, step } = fixture();
  for (let i = 0; i < 60; i++) step(0);
  assert.equal(dust.mesh.count, 0);
  for (let i = 0; i < 60; i++) step(12, 1 / 60, 2);
  assert.equal(dust.mesh.count, 0);
  step(1000);
  assert.equal(dust.mesh.count, 0);
  sim.human.alive = false;
  for (let i = 0; i < 30; i++) step(12);
  assert.equal(dust.mesh.count, 0);
  sim.dispose();
});

test("crowded scenes stay within the pool and recover after saturation", () => {
  const { sim, dust, step } = fixture(30, "quarry");
  let peak = 0;
  for (let i = 0; i < 120; i++) {
    step(30);
    peak = Math.max(peak, dust.mesh.count);
    assert.ok(dust.mesh.count <= TRACK_DUST_CAPACITY);
  }
  assert.equal(peak, TRACK_DUST_CAPACITY);
  for (let i = 0; i < 70; i++) step(0);
  assert.equal(dust.mesh.count, 0);
  for (let i = 0; i < 60; i++) step(30);
  assert.ok(dust.mesh.count > 0);
  sim.dispose();
});

test("village grass emits no dust while dirt roads and the other maps still do", () => {
  for (const map of ["village", "quarry", "harbor"] as const) {
    const { sim, dust, step } = fixture(1, map);
    sim.human.body.setTranslation({ x: 20, y: 0.65, z: 12 }, true);
    step(0);
    for (let i = 0; i < 45; i++) step(12);
    if (map === "village") {
      assert.equal(dust.mesh.count, 0, "grass stays clean");
      sim.human.body.setTranslation({ x: 0, y: 0.65, z: 12 }, true);
      step(0);
      for (let i = 0; i < 30; i++) step(12);
      assert.ok(dust.mesh.count > 0, "dirt road raises dust");
      sim.human.body.setTranslation({ x: 20, y: 0.65, z: 12 }, true);
      step(0);
      for (let i = 0; i < 45; i++) step(12);
      assert.equal(dust.mesh.count, 0, "existing dust fades after leaving the road");
    } else {
      assert.ok(dust.mesh.count > 0, `${map} keeps its dust`);
    }
    sim.dispose();
  }
});

test("each track checks its own surface when straddling the village road edge", () => {
  for (const speed of [12, -12]) {
    const { sim, dust, step } = fixture();
    sim.human.body.setTranslation({ x: 8.3, y: 0.65, z: 20 }, true);
    step(0);
    for (let i = 0; i < 30; i++) step(speed);
    assert.ok(dust.mesh.count > 0);
    const matrix = new Matrix4();
    for (let i = 0; i < dust.mesh.count; i++) {
      dust.mesh.getMatrixAt(i, matrix);
      assert.ok(matrix.elements[12] < 8.3, "only the track on dirt emits");
    }
    sim.dispose();
  }
});

test("hard turns keep dust proportional to driving and pivots emit less than fast travel", () => {
  const straight = fixture(1, "quarry");
  const turning = fixture(1, "quarry");
  const pivot = fixture(1, "quarry");
  let straightTotal = 0;
  let turningTotal = 0;
  let pivotTotal = 0;
  for (let i = 0; i < 120; i++) {
    straight.step(12);
    turning.sim.human.heading += 2.4 / 60;
    turning.step(12);
    pivot.sim.human.heading += 2.4 / 60;
    pivot.step(0);
    straightTotal += straight.dust.mesh.count;
    turningTotal += turning.dust.mesh.count;
    pivotTotal += pivot.dust.mesh.count;
  }
  assert.ok(turningTotal <= straightTotal * 1.2, "turning must not multiply dust density");
  assert.ok(pivotTotal > 0 && pivotTotal < straightTotal * 0.4);
  for (const f of [straight, turning, pivot]) f.sim.dispose();
});

test("turns on village grass and the stress-test grass floor stay clean", () => {
  for (const customGrass of [false, true]) {
    const { sim, dust, step } = fixture(1, "village", customGrass);
    if (!customGrass) {
      sim.human.body.setTranslation({ x: 20, y: 0.65, z: 20 }, true);
      step(0);
    }
    for (let i = 0; i < 60; i++) {
      sim.human.heading += 2.4 / 60;
      step(0);
    }
    assert.equal(dust.mesh.count, 0);
    assert.equal(dust.gravel.mesh.count, 0);
    sim.dispose();
  }
});

test("gravel is quarry-only, bounded, frozen on pause, and expires or resets", () => {
  for (const map of ["village", "quarry", "harbor"] as const) {
    const { sim, dust, step } = fixture(30, map);
    let peak = 0;
    for (let i = 0; i < 120; i++) {
      for (const tank of sim.tanks) tank.heading += 2.4 / 60;
      step(12);
      peak = Math.max(peak, dust.gravel.mesh.count);
      assert.ok(dust.gravel.mesh.count <= TRACK_GRAVEL_CAPACITY);
    }
    assert.equal(peak, map === "quarry" ? TRACK_GRAVEL_CAPACITY : 0);
    const matrices = dust.gravel.mesh.instanceMatrix.array.slice();
    for (let i = 0; i < 30; i++) dust.update(sim);
    assert.deepEqual(dust.gravel.mesh.instanceMatrix.array, matrices);
    for (let i = 0; i < 70; i++) step(0);
    assert.equal(dust.gravel.mesh.count, 0);
    for (let i = 0; i < 30; i++) step(12);
    dust.reset();
    assert.equal(dust.gravel.mesh.count, 0);
    sim.dispose();
  }
});

test("stationary pivots leave curved track marks without filling the pool at rest", () => {
  const { sim, step } = fixture();
  const trails = new TrackTrails();
  trails.update(sim, 1);
  for (let i = 0; i < 60; i++) {
    sim.human.heading += 2.4 / 60;
    step(0);
    trails.update(sim, 1);
  }
  const count = trails.mesh.count;
  assert.ok(count > 4);
  for (let i = 0; i < 60; i++) {
    step(0);
    trails.update(sim, 1);
  }
  assert.equal(trails.mesh.count, count);
  trails.dispose();
  sim.dispose();
});
