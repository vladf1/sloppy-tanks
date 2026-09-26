import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { InstancedBufferAttribute, Matrix4, Vector3 } from "three";
import { Simulation } from "../src/game/simulation";
import {
  HUMVEE_TRACK_STRENGTH,
  TRACK_CAPACITY,
  TRACK_LIFETIME,
  TrackTrails,
} from "../src/game/tracks";
import { VEHICLES } from "../src/game/data";
import type { VehicleKind } from "../src/game/types";
import { clearArena } from "./fixtures";

before(async () => {
  await RAPIER.init();
});

/** One human tank at the origin on an otherwise empty map, plus a trail renderer. */
function single(kind: VehicleKind = "balanced") {
  const sim = clearArena(new Simulation(123));
  const tank = sim.addTank(0, true, kind);
  tank.heading = 0;
  tank.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
  tank.previous = { x: 0, z: 0 };
  sim.start();
  const trails = new TrackTrails();
  const move = (x: number, z = 0) => {
    tank.body.setTranslation({ x, y: 0.65, z }, true);
    trails.update(sim, 1);
  };
  const dispose = () => {
    trails.dispose();
    sim.dispose();
  };
  return { sim, tank, trails, move, dispose };
}

/** A tank driving east past team 0's middle spawn pad. */
function padFixture(map: "village" | "quarry") {
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
  const dt = 1 / 60;
  for (let i = 0; i < 120 && tank.body.translation().x < -48; i++) {
    sim.elapsed += dt;
    const p = tank.body.translation();
    tank.body.setTranslation({ x: p.x + 8 * dt, y: 0.65, z: -23 }, true);
    tracks.update(sim, 1);
  }
  return { sim, tank, tracks, marks };
}

test("quarry prints ride on top of spawn pads and stay on the dirt elsewhere", () => {
  const f = padFixture("quarry");
  try {
    const marks = f.marks();
    const pad = marks.filter((m) => m.onPad);
    const dirt = marks.filter((m) => !m.onPad);
    assert.ok(pad.length > 0, "crossing the pad leaves prints");
    assert.ok(dirt.length > 0, "approach leaves prints too");
    for (const m of pad) assert.ok(Math.abs(m.y - 0.16) < 1e-6, `buried pad print at ${m.y}`);
    for (const m of dirt) assert.ok(Math.abs(m.y - 0.075) < 1e-6, `floating dirt print at ${m.y}`);
    const village = padFixture("village");
    try {
      assert.ok(village.marks().length > 0);
      for (const m of village.marks())
        assert.ok(Math.abs(m.y - 0.075) < 1e-6, "other maps stay flat");
    } finally {
      village.tracks.dispose();
      village.sim.dispose();
    }
  } finally {
    f.tracks.dispose();
    f.sim.dispose();
  }
});

test("tracks are distance-spaced at 30 and 120 FPS and skip stationary tanks and teleports", () => {
  const counts: number[] = [];
  for (const fps of [30, 120]) {
    const f = single();
    try {
      for (let i = 0; i <= fps * 2; i++) {
        const x = (i * 6) / fps;
        f.tank.previous = { x, z: 0 };
        f.sim.elapsed = i / fps;
        f.move(x);
      }
      const count = f.trails.mesh.count;
      counts.push(count);
      for (let i = 0; i < 10; i++) f.trails.update(f.sim, 1);
      assert.equal(f.trails.mesh.count, count, "a stationary tank adds nothing");
      f.move(50);
      assert.equal(f.trails.mesh.count, count, "a teleport adds nothing");
    } finally {
      f.dispose();
    }
  }
  assert.equal(counts[0], counts[1]);
});

test("stationary pivots leave curved track marks without filling the pool at rest", () => {
  const f = single();
  try {
    f.trails.update(f.sim, 1);
    for (let i = 0; i < 60; i++) {
      f.tank.heading += 2.4 / 60;
      f.sim.elapsed += 1 / 60;
      f.trails.update(f.sim, 1);
    }
    const count = f.trails.mesh.count;
    assert.ok(count > 4);
    for (let i = 0; i < 60; i++) {
      f.sim.elapsed += 1 / 60;
      f.trails.update(f.sim, 1);
    }
    assert.equal(f.trails.mesh.count, count);
  } finally {
    f.dispose();
  }
});

test("a full track buffer keeps still-visible marks, reuses faded ones and empties on reset", () => {
  const f = single();
  try {
    const fillSteps = TRACK_CAPACITY / 2;
    for (let i = 0; i < fillSteps; i++) f.move(i);
    assert.equal(f.trails.mesh.count, TRACK_CAPACITY);
    const before = f.trails.mesh.instanceMatrix.array.slice();
    for (let i = fillSteps; i < fillSteps + 10; i++) f.move(i);
    assert.deepEqual(f.trails.mesh.instanceMatrix.array, before);
    f.sim.elapsed = TRACK_LIFETIME + 0.1;
    f.move(fillSteps + 10);
    assert.notDeepEqual(f.trails.mesh.instanceMatrix.array, before);
    f.trails.reset();
    assert.equal(f.trails.mesh.count, 0);
    f.trails.update(f.sim, 1);
    assert.equal(f.trails.mesh.count, 0, "the first pose after a reset is only a reference");
  } finally {
    f.dispose();
  }
});

test("track expiry compacts live marks and uploads only changed slots", () => {
  const { sim, trails, move, dispose } = single();
  try {
    const matrix = trails.mesh.instanceMatrix;
    const birth = trails.mesh.geometry.getAttribute("trackBirth");
    assert.ok(birth instanceof InstancedBufferAttribute);
    move(0);
    move(1);
    const oldCount = trails.mesh.count;
    sim.elapsed = 10;
    move(2);
    const liveCount = trails.mesh.count - oldCount;
    const expected = [];
    for (let i = oldCount; i < trails.mesh.count; i++) {
      expected.push(Array.from(matrix.array.slice(i * 16, (i + 1) * 16)).join(","));
    }
    matrix.clearUpdateRanges();
    birth.clearUpdateRanges();
    sim.elapsed = TRACK_LIFETIME - 0.001;
    trails.update(sim, 1);
    assert.equal(trails.mesh.count, oldCount + liveCount, "keep marks until completely faded");
    sim.elapsed = TRACK_LIFETIME;
    trails.update(sim, 1);
    assert.equal(trails.mesh.count, liveCount);
    const actual = [];
    for (let i = 0; i < trails.mesh.count; i++) {
      actual.push(Array.from(matrix.array.slice(i * 16, (i + 1) * 16)).join(","));
      assert.equal(birth.getX(i), 10, "moving a slot preserves its fade age");
    }
    assert.deepEqual(actual.sort(), expected.sort(), "expiry preserves every younger mark");
    assert.ok(matrix.updateRanges.reduce((n, r) => n + r.count, 0) <= oldCount * 16);
    assert.deepEqual(
      matrix.updateRanges.map((r) => ({ start: r.start / 16, count: r.count / 16 })),
      birth.updateRanges,
    );
    sim.elapsed = 10 + TRACK_LIFETIME;
    trails.update(sim, 1);
    assert.equal(trails.mesh.count, 0, "no expired instances remain in the draw");
    move(3);
    assert.ok(trails.mesh.count > 0, "new tracks resume after all previous marks expire");
    trails.reset();
    assert.equal(matrix.updateRanges.length, 0);
    assert.equal(birth.updateRanges.length, 0);
  } finally {
    dispose();
  }
});

test("track upload ranges stay bounded through multiple trail lifetimes", () => {
  const { sim, trails, move, dispose } = single();
  try {
    for (let i = 0; i < 3000; i++) {
      sim.elapsed = i / 60;
      move((i % 300) * 0.1 - 15);
      // One tank can append one span and relocate at most two expired marks.
      // Do not emulate WebGL consuming these lists: WebGPU retains the originals.
      assert.ok(trails.mesh.instanceMatrix.updateRanges.length <= 3);
      for (const name of ["trackBirth", "trackStrength"]) {
        const attribute = trails.mesh.geometry.getAttribute(name);
        assert.ok("updateRanges" in attribute && attribute.updateRanges.length <= 3);
      }
    }
    assert.ok(trails.mesh.count > 0);
  } finally {
    dispose();
  }
});

test("thirty boosted scouts keep laying fresh tracks through multiple buffer wraps", () => {
  const s = new Simulation(123);
  s.reset(30);
  const tracks = new TrackTrails();
  const speed = VEHICLES.scout.speed * 1.5;
  for (const tank of s.tanks) tank.kind = "scout";
  const birth = tracks.mesh.geometry.getAttribute("trackBirth");
  assert.ok(birth instanceof InstancedBufferAttribute);
  for (let frame = 0; frame <= 60 * 60; frame++) {
    s.elapsed = frame / 60;
    for (const [i, tank] of s.tanks.entries()) {
      tank.body.setTranslation({ x: s.elapsed * speed, y: 0.65, z: i * 3 }, true);
    }
    tracks.update(s, 1);
    // A live renderer clears upload ranges after submitting them each frame.
    tracks.mesh.instanceMatrix.clearUpdateRanges();
    birth.clearUpdateRanges();
    if (frame > 0 && frame % 60 === 0) {
      let fresh = 0;
      for (let i = 0; i < tracks.mesh.count; i++) {
        if (s.elapsed - birth.getX(i) < 1) fresh++;
      }
      assert.ok(fresh > 2500, `fresh trails stalled at ${s.elapsed}s: ${fresh} marks`);
    }
  }
  assert.ok(tracks.mesh.count > 50000 && tracks.mesh.count < TRACK_CAPACITY);
  for (let i = 0; i < tracks.mesh.count; i++) {
    assert.ok(s.elapsed - birth.getX(i) < TRACK_LIFETIME, "expired marks must not be submitted");
  }
  tracks.dispose();
  s.dispose();
});

test("HMMWVs leave denser, fainter continuous wheel trails than tracked vehicles", () => {
  const trail = (kind: "humvee" | "scout") => {
    const f = single(kind);
    try {
      for (let i = 0; i <= 60; i++) {
        f.tank.previous = { x: i * 0.5 - 0.5, z: 0 };
        f.sim.elapsed = i / 60;
        f.move(i * 0.5);
      }
      const strength = f.trails.mesh.geometry.getAttribute("trackStrength");
      return { count: f.trails.mesh.count, strength: strength.getX(0) };
    } finally {
      f.dispose();
    }
  };
  const humvee = trail("humvee"),
    scout = trail("scout");
  assert.ok(humvee.count > scout.count * 1.8);
  assert.ok(Math.abs(humvee.strength - HUMVEE_TRACK_STRENGTH) < 1e-6);
  assert.equal(scout.strength, 1);
});
