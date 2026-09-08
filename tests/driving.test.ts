import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { idleCommand, type VehicleKind } from "../src/game/types";
import { angleDelta, VEHICLES } from "../src/game/data";

before(async () => { await RAPIER.init(); });
function arena(kind: VehicleKind = "balanced", heading = 0) {
  const s = new Simulation(123);
  for (const c of s.covers) s.world.removeRigidBody(c.body);
  for (const t of s.tanks) s.world.removeRigidBody(t.body);
  s.covers = []; s.coverByCollider.clear(); s.tanks = []; s.pickups = [];
  const t = s.addTank(0, true, kind);
  t.heading = heading;
  t.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
  t.previous = { x: 0, z: 0 };
  s.world.step(); s.start();
  return s;
}
function drive(s: Simulation, angle: number, steps: number) {
  for (let i = 0; i < steps; i++)
    s.step({ ...idleCommand(), moveX: Math.sin(angle), moveZ: Math.cos(angle), aim: 0.7 });
}

test("opposite input brakes then reverses without turning the hull on any chassis", () => {
  for (const kind of ["scout", "balanced", "heavy"] as const)
    for (const heading of [0, Math.PI / 2, Math.PI, -Math.PI / 2]) {
      const s = arena(kind, heading), t = s.human;
      try {
        drive(s, heading, 30);
        drive(s, heading + Math.PI, 1);
        const v = t.body.linvel();
        assert.ok(v.x * Math.sin(heading) + v.z * Math.cos(heading) > 0,
          "velocity must brake before changing sign");
        drive(s, heading + Math.PI, 29);
        assert.ok(Math.abs(angleDelta(heading, t.heading)) < 1e-8);
        const reverse = t.body.linvel();
        const signedSpeed = reverse.x * Math.sin(heading) + reverse.z * Math.cos(heading);
        assert.ok(signedSpeed < -VEHICLES[kind].speed * 0.75);
        assert.ok(signedSpeed > -VEHICLES[kind].speed * 0.85);
      } finally { s.dispose(); }
    }
});

test("perpendicular input gradually turns from rest without strafing, then reaches full speed", () => {
  for (const heading of [0, Math.PI / 2, Math.PI, -Math.PI / 2]) for (const side of [-1, 1]) {
    const s = arena("balanced", heading), t = s.human, target = heading + side * Math.PI / 2;
    try {
      drive(s, target, 1);
      assert.ok(Math.abs(angleDelta(heading, t.heading)) < 0.07);
      assert.ok(angleDelta(heading, t.heading) * side > 0, "broadside input favors forward steering");
      assert.ok(Math.hypot(t.body.linvel().x, t.body.linvel().z) < 0.1);
      drive(s, target, 11);
      assert.ok(Math.abs(angleDelta(t.heading, target)) > 0.7, "still turning at 0.2 seconds");
      const v = t.body.linvel();
      assert.ok(Math.abs(v.x * Math.cos(t.heading) - v.z * Math.sin(t.heading)) < 0.05,
        "drive follows the hull instead of the requested direction");
      drive(s, target, 18);
      assert.ok(Math.abs(angleDelta(t.heading, target)) < 1e-8, "aligned within half a second");
      assert.ok(Math.hypot(t.body.linvel().x, t.body.linvel().z) > VEHICLES.balanced.speed * 0.98);
      assert.equal(t.aim, 0.7, "turret aim remains independent of steering");
    } finally { s.dispose(); }
  }
});

test("a moving right-angle turn sheds speed and traces an arc instead of changing direction instantly", () => {
  const s = arena(), t = s.human;
  try {
    drive(s, 0, 30);
    const start = { ...t.body.translation() };
    drive(s, Math.PI / 2, 1);
    assert.ok(t.body.linvel().z > 5, "retains momentum on the first turning tick");
    assert.ok(t.body.linvel().x < 0.1, "does not immediately accelerate sideways");
    drive(s, Math.PI / 2, 9);
    assert.ok(Math.hypot(t.body.linvel().x, t.body.linvel().z) < VEHICLES.balanced.speed * 0.5);
    drive(s, Math.PI / 2, 20);
    const end = t.body.translation();
    assert.ok(end.x - start.x > 1 && end.z - start.z > 0.3);
    assert.ok(end.z - start.z < 1.5, "turn remains compact enough for responsive controls");
  } finally { s.dispose(); }
});

test("a diagonal behind the hull makes a short reverse turn across the angle wrap", () => {
  const s = arena("balanced", Math.PI - 0.1), t = s.human;
  try {
    const start = t.heading;
    drive(s, Math.PI / 4, 30);
    assert.ok(Math.abs(angleDelta(start, t.heading)) < Math.PI / 2);
    assert.ok(Math.abs(angleDelta(t.heading, Math.PI * 1.25)) < 1e-8);
    assert.ok(t.body.linvel().x > 0 && t.body.linvel().z > 0);
  } finally { s.dispose(); }
});

test("release brakes promptly and stops turning; external knockback is not erased", () => {
  const s = arena(), t = s.human;
  try {
    drive(s, Math.PI / 2, 10);
    const heading = t.heading;
    for (let i = 0; i < 12; i++) s.step();
    assert.equal(t.heading, heading);
    assert.ok(Math.hypot(t.body.linvel().x, t.body.linvel().z) < 0.01);
    t.body.applyImpulse({ x: t.body.mass() * 20, y: 0, z: 0 }, true);
    s.step();
    assert.ok(t.body.linvel().x > 15, "bounded braking preserves impact momentum");
  } finally { s.dispose(); }
});
