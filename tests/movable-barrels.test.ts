import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { idleCommand } from "../src/game/types";
import { blastDebris } from "../src/game/debris-physics";
import { stepProjectiles } from "../src/game/projectiles";
import { GROUP, STEP } from "../src/game/data";
import { clearArena } from "./fixtures";

before(async () => {
  await RAPIER.init();
});
function arena() {
  const sim = clearArena(new Simulation(123));
  sim.start();
  return sim;
}
function barrel(sim: Simulation, x = 0, z = 0) {
  return sim.addCover({ kind: "drum", x, z, w: 1.2, d: 1.2, h: 1.7, hp: 30, color: 0xff5b24 });
}

test("every tank can push barrels; hard shoves can tip and roll them before they settle and wake", () => {
  let rollingCases = 0;
  for (const kind of ["scout", "balanced", "heavy"] as const) {
    const sim = arena();
    try {
      const drum = barrel(sim);
      const tank = sim.addTank(0, true, kind);
      tank.heading = 0;
      tank.protection = 0;
      tank.body.setTranslation({ x: 0, y: 0.65, z: -5 }, true);
      const hp = tank.hp;
      let tipped = false;
      let rolled = false;
      for (let i = 0; i < 120 + 20 / STEP; i++) {
        sim.step(i < 120 ? { ...idleCommand(), moveZ: 1 } : idleCommand());
        const q = drum.body.rotation();
        const sideways = Math.abs(1 - 2 * (q.x * q.x + q.z * q.z)) < 0.4;
        tipped ||= sideways;
        const v = drum.body.linvel();
        const spin = drum.body.angvel();
        rolled ||= sideways && Math.hypot(v.x, v.z) > 0.5 && Math.hypot(spin.x, spin.z) > 0.5;
      }
      assert.ok(drum.z > 3, `${kind} pushes the barrel`);
      if (tipped && rolled) rollingCases++;
      assert.equal(drum.hp, 30);
      assert.equal(tank.hp, hp);
      assert.ok(drum.body.isSleeping(), `${kind}: barrel settles`);
      assert.equal(drum.body.numColliders(), 1);
      assert.equal(drum.collider.collisionGroups(), GROUP.movableCover);
      blastDebris(sim, { x: drum.x - 1, z: drum.z }, 4, 15);
      assert.equal(drum.body.isSleeping(), false);
      assert.ok(Math.hypot(...Object.values(drum.body.linvel())) > 0.5);
      assert.equal(drum.hp, 30);
    } finally {
      sim.dispose();
    }
  }
  assert.ok(rollingCases >= 2, "faster shoves tip barrels into a physical roll");
});

test("a displaced tipped barrel updates navigation and can be shot to chain-explode at its new position", () => {
  const sim = arena();
  try {
    const drum = barrel(sim);
    sim.nav.rebuild(sim.covers);
    const old = sim.nav.index({ x: 0, z: 0 });
    assert.equal(sim.nav.blocked[old], 1);
    drum.body.setTranslation({ x: 12, y: 0.61, z: 0 }, true);
    // Lay its cylinder axis along X.
    drum.body.setRotation({ x: 0, y: 0, z: Math.SQRT1_2, w: Math.SQRT1_2 }, true);
    barrel(sim, 15, 0);
    sim.step();
    assert.equal(sim.nav.blocked[old], 0);
    assert.equal(sim.nav.blocked[sim.nav.index(drum)], 1);
    const position = drum.body.translation();
    const handle = drum.collider.handle;
    sim.shots.push({
      id: sim.nextId++,
      owner: 999,
      team: 0,
      x: 8,
      z: 0,
      y: 1,
      vx: 25,
      vz: 0,
      damage: 40,
      bounces: 0,
      life: 2,
      weapon: "standard",
      piercing: 0,
    });
    stepProjectiles(sim, 0.2);
    assert.equal(drum.alive, false);
    assert.equal(drum.body.isValid(), false);
    assert.equal(sim.coverByCollider.has(handle), false);
    assert.equal(sim.destroyed, 2);
    const explosions = sim.events.filter((e) => e.type === "explosion" && e.coverKind === "drum");
    assert.equal(explosions.length, 2);
    assert.ok(Math.abs(explosions[0].x - position.x) < 0.01);
    assert.ok(explosions.every((e) => e.x > 10));
    const lid = sim.fragments.find((f) => f.shape === "drum-lid")!;
    assert.ok(
      Math.abs(lid.dimensions!.x - 0.78) < 0.001,
      "navigation bounds never inflate the lid",
    );
    assert.ok(
      Math.abs(lid.body.translation().x - (position.x - 0.85)) < 0.02,
      "debris follows the tipped body pose",
    );
    assert.doesNotThrow(() => {
      for (let i = 0; i < 120; i++) sim.step();
    });
  } finally {
    sim.dispose();
  }
});

test("destroying a barrel between navigation updates clears its old footprint", () => {
  const sim = arena();
  try {
    const drum = barrel(sim);
    sim.nav.rebuild(sim.covers);
    const old = sim.nav.index(drum);
    drum.body.setTranslation({ x: 8, y: 0.85, z: 0 }, true);
    // Destroy before updateMovableCover has synchronized its location or navigation.
    sim.damageCover(drum, 40, 999, 0);
    assert.equal(sim.nav.blocked[old], 0);
    assert.equal(sim.events.find((e) => e.type === "explosion")!.x, 8);
  } finally {
    sim.dispose();
  }
});
