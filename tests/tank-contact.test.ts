import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Box3 } from "three";
import { Simulation } from "../src/game/simulation";
import { tankModel } from "../src/game/models";
import { STEP, VEHICLES, GROUP, MOVE_ACCELERATION } from "../src/game/data";

before(async () => { await RAPIER.init(); });

test("boosted tanks stop at visible hull edges in head-on and side contacts, including rotated hulls", () => {
  for (const kind of ["scout", "balanced", "heavy"] as const)
    for (const axis of ["x", "z"] as const) for (const angle of [0, Math.PI / 3]) for (const team of [0, 1] as const) {
      const s = new Simulation(123);
      for (const t of s.tanks) s.world.removeRigidBody(t.body);
      s.tanks = [];
      for (const c of s.covers) s.world.removeRigidBody(c.body);
      s.covers = [];
      s.addTank(0, true, kind); s.addTank(team, true, kind);
      const [a, b] = s.tanks;
      const model = tankModel(kind, 0); model.updateMatrixWorld(true);
      const bounds = new Box3().setFromObject(model.userData.hull);
      const expected = bounds.max[axis] - bounds.min[axis];
      const direction = axis === "x"
        ? { x: Math.cos(angle), z: -Math.sin(angle) }
        : { x: Math.sin(angle), z: Math.cos(angle) };
      for (const [i, t] of s.tanks.entries()) {
        const side = i === 0 ? -1 : 1;
        t.body.setTranslation({ x: direction.x * side * 4, y: 0.65, z: direction.z * side * 4 }, true);
        t.body.setRotation({ x: 0, y: Math.sin(angle / 2), z: 0, w: Math.cos(angle / 2) }, true);
        t.body.lockRotations(true, true);
        assert.ok(Math.abs(t.body.mass() - VEHICLES[kind].mass) < 0.001);
      }
      let minimum = Infinity;
      for (let frame = 0; frame < 120; frame++) {
        for (const [i, t] of s.tanks.entries()) {
          const speed = VEHICLES[kind].speed * 1.5 * (i === 0 ? 1 : -1);
          const v = t.body.linvel();
          const dx = direction.x * speed - v.x, dz = direction.z * speed - v.z;
          const amount = Math.min(1, MOVE_ACCELERATION * STEP / (Math.hypot(dx, dz) || 1));
          t.body.applyImpulse({ x: dx * amount * t.body.mass(), y: 0, z: dz * amount * t.body.mass() }, true);
        }
        s.world.timestep = STEP; s.world.step();
        const p = a.body.translation(), q = b.body.translation();
        minimum = Math.min(minimum, (q.x - p.x) * direction.x + (q.z - p.z) * direction.z);
      }
      assert.ok(minimum >= expected - 0.01, `${kind} ${axis} ${angle}: ${minimum} versus ${expected}`);
      s.dispose();
    }
});

test("model-sized contact collider is recreated on class-changing respawn and cleaned on death", () => {
  const s = new Simulation(123), t = s.human;
  const count = s.world.colliders.len();
  const restitution = t.collider.restitution(), friction = t.collider.friction();
  t.protection = 0;
  s.damageTank(t, 1000, 999, 1);
  s.humanKind = "heavy";
  s.respawn(t);
  assert.equal(t.body.numColliders(), 2);
  assert.equal(t.collider.restitution(), restitution, "respawn preserves hull bounce");
  assert.equal(t.collider.friction(), friction, "respawn preserves hull friction");
  const contact = t.body.collider(1);
  assert.equal(contact.collisionGroups(), GROUP.tankContact);
  const model = tankModel("heavy", 0); model.updateMatrixWorld(true);
  const bounds = new Box3().setFromObject(model.userData.hull);
  assert.ok(Math.abs(contact.halfExtents()!.z * 2 - (bounds.max.z - bounds.min.z)) < 0.00001);
  assert.ok(Math.abs(t.body.mass() - VEHICLES.heavy.mass) < 0.001);
  s.reset();
  assert.equal(s.world.colliders.len(), count);
  s.dispose();
});

test("hull proportions preserve elongated reference-style silhouettes, including tracks and skirts", () => {
  for (const [kind, ratio] of [["scout", 1.94], ["balanced", 2.17], ["heavy", 2.17]] as const) {
    const model = tankModel(kind, 0);
    model.updateMatrixWorld(true);
    const bounds = new Box3().setFromObject(model.userData.hull);
    const actual = (bounds.max.z - bounds.min.z) / (bounds.max.x - bounds.min.x);
    assert.ok(Math.abs(actual - ratio) < 0.04, `${kind}: ${actual} versus reference ${ratio}`);
  }
});

test("different chassis meeting at right angles cannot overlap their visible hulls", () => {
  for (const aKind of ["scout", "balanced", "heavy"] as const)
    for (const bKind of ["scout", "balanced", "heavy"] as const) {
      const s = new Simulation(123);
      for (const t of s.tanks) s.world.removeRigidBody(t.body);
      for (const c of s.covers) s.world.removeRigidBody(c.body);
      s.tanks = []; s.covers = [];
      s.addTank(0, true, aKind); s.addTank(1, true, bKind);
      const [a, b] = s.tanks;
      let expected = 0;
      for (const [i, t] of s.tanks.entries()) {
        const model = tankModel(t.kind, t.team);
        model.rotation.y = i * Math.PI / 2;
        model.updateMatrixWorld(true);
        const bounds = new Box3().setFromObject(model.userData.hull);
        expected += i === 0 ? bounds.max.z : -bounds.min.z;
        t.body.setTranslation({ x: 0, y: 0.65, z: i === 0 ? -6 : 6 }, true);
        t.body.setRotation({ x: 0, y: Math.sin(i * Math.PI / 4), z: 0, w: Math.cos(i * Math.PI / 4) }, true);
        t.body.lockRotations(true, true);
      }
      let minimum = Infinity;
      for (let frame = 0; frame < 180; frame++) {
        for (const [i, t] of s.tanks.entries()) {
          const target = VEHICLES[t.kind].speed * 1.5 * (i === 0 ? 1 : -1);
          const change = Math.max(-MOVE_ACCELERATION * STEP, Math.min(MOVE_ACCELERATION * STEP, target - t.body.linvel().z));
          t.body.applyImpulse({ x: 0, y: 0, z: change * t.body.mass() }, true);
        }
        s.world.timestep = STEP; s.world.step();
        minimum = Math.min(minimum, b.body.translation().z - a.body.translation().z);
      }
      assert.ok(minimum >= expected - 0.015, `${aKind}/${bKind}: ${minimum} versus ${expected}`);
      s.dispose();
    }
});

test("long hulls stop at walls using their visible nose and tail", () => {
  for (const kind of ["scout", "balanced", "heavy"] as const) for (const side of [-1, 1]) {
    const s = new Simulation(123);
    for (const t of s.tanks) s.world.removeRigidBody(t.body);
    for (const c of s.covers) s.world.removeRigidBody(c.body);
    s.tanks = []; s.covers = [];
    s.addTank(0, true, kind);
    const t = s.tanks[0];
    t.body.setTranslation({ x: 0, y: 0.65, z: -side * 7 }, true);
    t.body.lockRotations(true, true);
    s.addCover({ kind: "concrete", x: 0, z: 0, w: 20, d: 0.5, h: 3, hp: Infinity, color: 0 });
    const model = tankModel(kind, 0); model.updateMatrixWorld(true);
    const bounds = new Box3().setFromObject(model.userData.hull);
    const reach = side === 1 ? bounds.max.z : -bounds.min.z;
    for (let frame = 0; frame < 180; frame++) {
      const change = Math.max(-MOVE_ACCELERATION * STEP, Math.min(MOVE_ACCELERATION * STEP,
        VEHICLES[kind].speed * side * 1.5 - t.body.linvel().z));
      t.body.applyImpulse({ x: 0, y: 0, z: change * t.body.mass() }, true);
      s.world.timestep = STEP; s.world.step();
      assert.ok(t.body.translation().z * side + reach <= -0.25 + 0.015, `${kind}/${side} clipped wall`);
    }
    s.dispose();
  }
});

test("reference MBTs use comparable widths instead of exaggerated class-size multipliers", () => {
  const widths = (["scout", "balanced", "heavy"] as const).map(kind => {
    const model = tankModel(kind, 0); model.updateMatrixWorld(true);
    const bounds = new Box3().setFromObject(model.userData.hull);
    return bounds.max.x - bounds.min.x;
  });
  assert.ok(Math.max(...widths) / Math.min(...widths) < 1.06);
  assert.ok(widths.every(width => width > 1.8 && width < 2));
});
