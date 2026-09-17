import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Matrix4, Vector3 } from "three";
import { Simulation } from "../src/game/simulation";
import { TrackDust, TRACK_DUST_CAPACITY } from "../src/game/track-dust";

before(async () => {
  await RAPIER.init();
});

function fixture(count = 1, map: "village" | "quarry" | "harbor" = "village") {
  const sim = new Simulation(123);
  sim.mapMode = map;
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
