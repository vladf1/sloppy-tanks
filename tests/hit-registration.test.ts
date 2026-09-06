import { before, test } from "node:test";
import assert from "node:assert/strict";
import * as THREE from "three";
import { tankModel } from "../src/game/models";
import { SHELL_HIT_RADIUS, tankHitTime } from "../src/game/hitboxes";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { stepProjectiles } from "../src/game/weapons";
import { VEHICLES, STEP } from "../src/game/data";
import type { VehicleKind } from "../src/game/types";
before(async () => { await RAPIER.init(); });
function fixture(kind: VehicleKind = "balanced", heading = 0) {
  const s = new Simulation(123);
  const target = s.tanks.find((t) => !t.human && t.kind === kind)!;
  for (const t of s.tanks) if (t !== target) s.world.removeRigidBody(t.body);
  s.tanks = [target];
  for (const c of s.covers) s.world.removeRigidBody(c.body);
  s.covers = []; s.pickups = []; s.nav.rebuild([]);
  target.team = 1; target.protection = 0; target.heading = heading;
  target.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
  target.body.setRotation({ x: 0, y: Math.sin(heading / 2), z: 0, w: Math.cos(heading / 2) }, true);
  target.previous = { x: 0, z: 0 };
  target.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
  target.body.setAngvel({ x: 0, y: 0, z: 0 }, true);
  s.world.step(); s.start();
  return { s, target };
}
function shell(s: Simulation, x: number, z: number, vx: number, vz: number) {
  s.shots.push({ id: s.nextId++, x, z, vx, vz, owner: 999, team: 0,
    damage: 40, bounces: 0, life: 2, weapon: "standard" });
}

test("hits on the visible outer tracks register for every chassis and rotated hull", () => {
  for (const kind of ["scout", "balanced", "heavy"] as const) for (const angle of [0, Math.PI / 3]) {
    const { s, target } = fixture(kind, angle), scale = VEHICLES[kind].scale;
    // Cross the outer tread, outside the old 0.83 * scale physics half-width.
    const x = 1.15 * scale, z = -5;
    const sin = Math.sin(angle), cos = Math.cos(angle);
    shell(s, cos * x + sin * z, -sin * x + cos * z, sin * 600, cos * 600);
    stepProjectiles(s, STEP);
    assert.equal(target.hp, VEHICLES[kind].health - 40, `${kind} at ${angle}`);
    assert.equal(s.shots.length, 0);
    s.dispose();
  }
});

test("visible nose and rear armor register, including the heavy's longer hull", () => {
  for (const kind of ["balanced", "heavy"] as const) for (const side of [-1, 1]) {
    const { s, target } = fixture(kind);
    const z = (kind === "heavy" ? 1.62 : 1.32) * VEHICLES[kind].scale * side;
    shell(s, -5, z, 600, 0); stepProjectiles(s, STEP);
    assert.equal(target.hp, VEHICLES[kind].health - 40, `${kind} ${side}`);
    s.dispose();
  }
});

test("shell radius grazes count, but a clean gap beyond the visible hull remains a miss", () => {
  for (const [x, expected] of [[1.38, 60], [1.7, 100]]) {
    const { s, target } = fixture();
    shell(s, x, -5, 0, 600); stepProjectiles(s, STEP);
    assert.equal(target.hp, expected, `offset ${x}`);
    s.dispose();
  }
});

test("cover still blocks shots at the widened hull, and protected targets do not lose health", () => {
  const { s, target } = fixture();
  s.addCover({ kind: "concrete", x: 0, z: -2.5, w: 5, d: 0.25, h: 3, hp: Infinity, color: 0 });
  s.world.step();
  shell(s, 1.15, -5, 0, 600); stepProjectiles(s, STEP);
  assert.equal(target.hp, 100); assert.equal(s.shots.length, 0);
  for (const c of s.covers) s.world.removeRigidBody(c.body); s.covers = [];
  target.protection = 1;
  shell(s, 0, -5, 0, 600); stepProjectiles(s, STEP);
  assert.equal(target.hp, 100);
  target.protection = 0; target.shield = 10; target.shieldPoints = 40;
  shell(s, 0, -5, 0, 600); stepProjectiles(s, STEP);
  assert.equal(target.hp, 100); assert.equal(target.shieldPoints, 0);
  s.dispose();
});


test("combat hit boundaries match rendered hull bounds plus shell radius on every side", () => {
  for (const kind of ["scout", "balanced", "heavy"] as const) {
    const { s, target } = fixture(kind), scale = VEHICLES[kind].scale;
    const model = tankModel(kind, 1); model.updateMatrixWorld(true);
    const bounds = new THREE.Box3().setFromObject(model.userData.hull);
    // Check both hits and misses immediately around the model-derived boundary.
    for (const axis of ["x", "z"] as const) for (const side of [-1, 1]) {
      const edge = (side < 0 ? bounds.min[axis] : bounds.max[axis]) + side * SHELL_HIT_RADIUS;
      for (const delta of [-0.001, 0.001]) {
        const offset = edge + side * delta;
        shell(s, axis === "x" ? offset : -5, axis === "z" ? offset : -5,
          axis === "x" ? 0 : 600, axis === "z" ? 0 : 600);
        const hit = tankHitTime(s.shots.pop()!, target, STEP);
        assert.equal(hit !== null, delta < 0, `${kind} ${axis} ${side} ${delta}`);
      }
    }
    const physical = target.collider.halfExtents();
    assert.ok(physical);
    assert.ok(Math.abs(physical.x - 0.83 * scale) < 1e-6);
    s.dispose();
  }
});

test("moving tanks are hit at the crossing time, not just their end-of-tick location", () => {
  for (const [start, end, expected] of [[-5, 5, 60], [-6, 0, 100]]) {
    const { s, target } = fixture();
    target.body.setTranslation({ x: 0, y: 0.65, z: end }, true);
    target.previous = { x: 0, z: start };
    shell(s, -5, 0, 600, 0);
    stepProjectiles(s, STEP, true);
    assert.equal(target.hp, expected, `travel ${start} to ${end}`);
    s.dispose();
  }
});

test("allied outer tracks remain transparent to shells", () => {
  const { s, target } = fixture(); target.team = 0;
  shell(s, 1.15, -5, 0, 600);
  assert.equal(tankHitTime(s.shots[0], target, STEP), null);
  stepProjectiles(s, STEP);
  assert.equal(target.hp, 100); assert.equal(s.shots.length, 1);
  s.dispose();
});
