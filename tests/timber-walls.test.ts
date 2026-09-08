import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { stepProjectiles } from "../src/game/weapons";
import { STEP } from "../src/game/data";

before(async () => { await RAPIER.init(); });

test("three shells breach one timber bay, clearing physics and bot navigation; reset restores it", () => {
  const s = new Simulation(123);
  try {
    for (const t of s.tanks) { s.world.removeRigidBody(t.body); }
    s.tanks = [];
    const wall = s.covers.find(c => c.kind === "timber" && c.x === -2 && c.z === 13)!;
    const neighbor = s.covers.find(c => c.kind === "timber" && c.x === 2 && c.z === 13)!;
    assert.ok(wall && neighbor);
    const handle = wall.collider.handle, version = s.nav.version;
    assert.equal(s.nav.blocked[s.nav.index(wall)], 1);
    for (let hit = 1; hit <= 3; hit++) {
      s.shots = [{ id: s.nextId++, x: wall.x, z: wall.z - 5, vx: 0, vz: 600,
        owner: 999, team: 0, damage: 40, bounces: 0, life: 2, piercing: 0, weapon: "standard" }];
      stepProjectiles(s, STEP);
      assert.equal(wall.hp, 120 - hit * 40);
      assert.equal(wall.alive, hit < 3);
      assert.equal(s.shots.length, 0);
    }
    assert.equal(neighbor.hp, 120);
    assert.equal(neighbor.alive, true);
    assert.equal(s.coverByCollider.has(handle), false);
    assert.ok(s.nav.version > version);
    assert.equal(s.nav.blocked[s.nav.index(wall)], 0);
    s.world.step();
    assert.equal(s.visible({ x: wall.x, z: wall.z - 3 }, { x: wall.x, z: wall.z + 3 }), true);
    assert.ok(s.fragments.length > 0);
    s.reset();
    assert.ok(s.covers.filter(c => c.kind === "timber").every(c => c.alive && c.hp === 120));
  } finally { s.dispose(); }
});

test("blast can destroy adjacent timber bays without duplicate destruction", () => {
  const s = new Simulation(123);
  try {
    const bays = s.covers.filter(c => c.kind === "timber" && c.z === 13 && Math.abs(c.x) === 2);
    s.explode({ x: 0, z: 13 }, 5, 120, s.human.id, s.humanTeam);
    for (const bay of bays) {
      assert.equal(bay.alive, false);
      s.damageCover(bay, 999, s.human.id, s.humanTeam);
      assert.equal(s.events.filter(e => e.type === "destroy" && e.id === bay.id).length, 1);
    }
  } finally { s.dispose(); }
});
