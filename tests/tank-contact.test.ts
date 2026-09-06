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
  t.protection = 0;
  s.damageTank(t, 1000, 999, 1);
  s.humanKind = "heavy";
  s.respawn(t);
  assert.equal(t.body.numColliders(), 2);
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
