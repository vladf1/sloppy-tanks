import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Matrix4, Vector3 } from "three";
import { Simulation } from "../src/game/simulation";
import { TrackTrails } from "../src/game/tracks";

before(async () => {
  await RAPIER.init();
});

function fixture(map: "village" | "quarry") {
  const sim = new Simulation(123);
  sim.mapMode = map;
  sim.reset(2);
  for (const tank of sim.tanks) sim.world.removeRigidBody(tank.body);
  sim.tanks = [];
  const tank = sim.addTank(0, true, "balanced");
  tank.heading = Math.PI / 2;
  tank.body.setTranslation({ x: -58, y: 0.65, z: -23 }, true);
  sim.start();
  const tracks = new TrackTrails();
  const position = new Vector3();
  const matrix = new Matrix4();
  const marks = () => {
    const heights: { y: number; onPad: boolean }[] = [];
    for (let i = 0; i < tracks.mesh.count; i++) {
      tracks.mesh.getMatrixAt(i, matrix);
      position.setFromMatrixPosition(matrix);
      heights.push({
        y: position.y,
        onPad: Math.hypot(position.x + 53, position.z + 23) < 2.75,
      });
    }
    return heights;
  };
  return { sim, tank, tracks, marks };
}

function drive(f: ReturnType<typeof fixture>, toX: number) {
  const dt = 1 / 60;
  for (let i = 0; i < 120 && f.tank.body.translation().x < toX; i++) {
    f.sim.elapsed += dt;
    const p = f.tank.body.translation();
    f.tank.body.setTranslation({ x: p.x + 8 * dt, y: 0.65, z: -23 }, true);
    f.tracks.update(f.sim, 1);
  }
}

test("quarry prints ride on top of spawn pads and stay on the dirt elsewhere", () => {
  const f = fixture("quarry");
  try {
    drive(f, -48);
    const marks = f.marks();
    const pad = marks.filter((m) => m.onPad);
    const dirt = marks.filter((m) => !m.onPad);
    assert.ok(pad.length > 0, "crossing the pad leaves prints");
    assert.ok(dirt.length > 0, "approach leaves prints too");
    for (const m of pad) assert.ok(Math.abs(m.y - 0.16) < 1e-6, `buried pad print at ${m.y}`);
    for (const m of dirt) assert.ok(Math.abs(m.y - 0.075) < 1e-6, `floating dirt print at ${m.y}`);
  } finally {
    f.sim.dispose();
  }
});

test("other maps keep every print on the dirt", () => {
  const f = fixture("village");
  try {
    drive(f, -48);
    const marks = f.marks();
    assert.ok(marks.length > 0);
    for (const m of marks) assert.ok(Math.abs(m.y - 0.075) < 1e-6);
  } finally {
    f.sim.dispose();
  }
});
