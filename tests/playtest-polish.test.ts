import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Vector3 } from "three";
import { Simulation } from "../src/game/simulation";
import { VEHICLES, WEAPONS, STEP } from "../src/game/data";
import { tankModel } from "../src/game/models";
import { fireWeapon, stepProjectiles } from "../src/game/weapons";
import { tuneSpeed } from "../src/game/speed-tuning";
import { shuffledBotNames } from "../src/game/bot-personalities";
import type { VehicleKind } from "../src/game/types";
before(async () => { await RAPIER.init(); });
function arena(kind: VehicleKind = "balanced") {
  const s = new Simulation(123);
  for (const t of s.tanks) s.world.removeRigidBody(t.body);
  for (const c of s.covers) s.world.removeRigidBody(c.body);
  s.tanks = []; s.covers = [];
  s.addTank(0, true, kind);
  const t = s.tanks[0];
  t.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
  t.aim = 0;
  return { s, t };
}
test("shells and spread pellets emerge from the model muzzle for all chassis and aim directions", () => {
  for (const kind of ["scout", "balanced", "heavy"] as const) for (const angle of [0, 1.2]) {
    const { s, t } = arena(kind);
    t.aim = angle; t.ammo.spread = 18; t.selectedAmmo = "spread";
    const model = tankModel(kind, 0); model.userData.turret.rotation.y = angle;
    model.position.y = 0.25; model.updateMatrixWorld(true);
    const muzzle = model.userData.muzzle.getWorldPosition(new Vector3());
    fireWeapon(s, t);
    assert.equal(s.shots.length, 3);
    for (const shot of s.shots) {
      assert.ok(Math.abs(shot.x - muzzle.x) < 1e-5);
      assert.ok(Math.abs(shot.z - muzzle.z) < 1e-5);
      assert.ok(Math.abs(shot.y! - muzzle.y) < 1e-5);
    }
    s.dispose();
  }
});
test("a protruding barrel cannot spawn shots beyond nearby cover or an enemy", () => {
  for (const obstruction of ["cover", "enemy"] as const) {
    const { s, t } = arena();
    if (obstruction === "cover") s.addCover({ kind: "fence", x: 0, z: 1, w: 3, d: 0.2, h: 2, hp: 100, color: 0 });
    else {
      s.addTank(1, true, "balanced");
      const target = s.tanks[1]; target.protection = 0;
      target.body.setTranslation({ x: 0, y: 0.65, z: 2 }, true);
    }
    s.world.step();
    t.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    if (obstruction === "enemy") s.tanks[1].body.setTranslation({ x: 0, y: 0.65, z: 2 }, true);
    s.world.propagateModifiedBodyPositionsToColliders();
    fireWeapon(s, t); stepProjectiles(s, STEP);
    assert.equal(obstruction === "cover" ? s.covers[0].hp : s.tanks[1].hp, 60, obstruction);
    s.dispose();
  }
});
test("speed sliders scale from defaults without compounding and update active shells and collision prediction", () => {
  const { s, t } = arena();
  const tankBase = VEHICLES.balanced.speed, shellBase = WEAPONS.standard.speed;
  try {
    fireWeapon(s, t);
    tuneSpeed(s, "tank-speed", 1.5); tuneSpeed(s, "tank-speed", 1.5);
    assert.equal(VEHICLES.balanced.speed, tankBase * 1.5);
    assert.ok(Math.abs(t.body.softCcdPrediction() - VEHICLES.balanced.speed * 1.5 * STEP * 2) < 1e-6);
    tuneSpeed(s, "bullet-speed", 0.5); tuneSpeed(s, "bullet-speed", 0.5);
    assert.equal(WEAPONS.standard.speed, shellBase * 0.5);
    assert.equal(s.shots[0].vz, shellBase * 0.5);
    assert.equal(tuneSpeed(s, "tank-speed", NaN), 1);
  } finally { tuneSpeed(s, "tank-speed", 1); tuneSpeed(s, "bullet-speed", 1); s.dispose(); }
});
test("large random name decks are unique, change each round and persist through respawn", () => {
  const deck = shuffledBotNames(123);
  assert.ok(deck.length >= 80); assert.equal(new Set(deck).size, deck.length);
  assert.deepEqual(deck, shuffledBotNames(123));
  assert.notDeepEqual(deck, shuffledBotNames(124));
  const s = new Simulation(123), t = s.tanks.find((t) => !t.human)!;
  const initial = s.tanks.filter((t) => !t.human).map((t) => t.name), name = t.name;
  assert.equal(new Set(initial).size, initial.length);
  t.protection = 0; s.damageTank(t, 1000, 999, (1 - t.team) as 0 | 1); s.respawn(t);
  assert.equal(t.name, name);
  s.reset(); assert.notDeepEqual(s.tanks.filter((t) => !t.human).map((t) => t.name), initial);
  s.dispose();
});
