import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { spawnPositions } from "../src/game/arena";
import { collectPickup, fireWeapon, interceptionTime, stepProjectiles } from "../src/game/weapons";
import { STEP, VEHICLES, WEAPONS, MOVE_ACCELERATION } from "../src/game/data";
import { TrackTrails, TRACK_CAPACITY } from "../src/game/tracks";
import { idleCommand, type Shot, type Team, type PickupKind } from "../src/game/types";
before(async () => { await RAPIER.init(); });
function arena(tanks = 0) {
  const s = new Simulation(123);
  for (const c of s.covers) s.world.removeRigidBody(c.body);
  s.covers = [];
  for (const t of s.tanks.slice(tanks)) s.world.removeRigidBody(t.body);
  s.tanks = s.tanks.slice(0, tanks);
  for (const [i, t] of s.tanks.entries()) {
    t.human = i === 0;
    t.protection = 0;
    t.body.setTranslation({ x: 0, y: 0.65, z: i * 12 }, true);
    t.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
    t.previous = { x: 0, z: i * 12 };
  }
  s.humanTeam = 0;
  s.pickups = [];
  s.nav.rebuild([]);
  s.world.step();
  s.start();
  return s;
}
function shot(s: Simulation, x: number, z: number, vx: number, vz: number, team: Team): Shot {
  return { id: s.nextId++, x, z, vx, vz, team, owner: s.tanks.find((t) => t.team === team)?.id ?? team,
    damage: 40, bounces: 0, life: 3.5, piercing: 0, weapon: "standard" };
}
function pickup(s: Simulation, kind: PickupKind) {
  collectPickup(s, s.human, { id: s.nextId++, x: 0, z: 0, kind, available: true, cooldown: 0 });
}
function wall(s: Simulation) {
  s.addCover({ kind: "concrete", x: 0, z: 0, w: 0.5, d: 8, h: 3, hp: Infinity, color: 0 });
  s.world.step();
}

test("opposing fast shells intercept between endpoints; allies and asynchronous crossing paths pass", () => {
  const s = arena();
  const a = shot(s, -5, 0, 1000, 0, 0), b = shot(s, 5, 0, -1000, 0, 1);
  s.shots = [a, b];
  stepProjectiles(s, STEP);
  assert.equal(s.shots.length, 0);
  assert.equal(s.events.filter((e) => e.type === "explosion").length, 1);
  assert.equal(interceptionTime(a, { ...b, team: 0 }, 1), null);
  const crossing = shot(s, 0, -8, 0, 100, 1);
  assert.equal(interceptionTime(shot(s, -2, 0, 100, 0, 0), crossing, 0.1), null);
  s.dispose();
});

test("the earliest interception consumes each bullet once, independent of array order", () => {
  for (const reversed of [false, true]) {
    const s = arena();
    const a = shot(s, 0, 0, 100, 0, 0), near = shot(s, 2, 0, 0, 0, 1), far = shot(s, 4, 0, 0, 0, 1);
    s.shots = reversed ? [far, near, a] : [a, near, far];
    stepProjectiles(s, 0.1);
    assert.deepEqual(s.shots.map((p) => p.id), [far.id]);
    assert.equal(s.events.filter((e) => e.type === "explosion").length, 1);
    s.dispose();
  }
});

test("a wall blocks interception, while a reflected shell can intercept on its new path", () => {
  const s = arena();
  wall(s);
  s.shots = [shot(s, -2, 0, 100, 0, 0), shot(s, 2, 0, -100, 0, 1)];
  stepProjectiles(s, 0.1);
  assert.equal(s.events.filter((e) => e.type === "explosion").length, 0);
  assert.equal(s.shots.length, 0);
  s.events = [];
  s.shots = [{ ...shot(s, -1, 0, 60, 0, 0), bounces: 1 }, shot(s, -3, 0, 60, 0, 1)];
  stepProjectiles(s, 0.05);
  assert.equal(s.events.filter((e) => e.type === "ricochet").length, 1);
  assert.equal(s.events.filter((e) => e.type === "explosion").length, 1);
  assert.equal(s.shots.length, 0);
  s.dispose();
});

test("earlier tank impacts and lifetime expiry take precedence over later interception", () => {
  const s = arena(1);
  const a = shot(s, -4, 0, 100, 0, 1), b = shot(s, 4, 0, -10, 0, 0);
  s.shots = [a, b];
  const hp = s.human.hp;
  stepProjectiles(s, 0.1);
  assert.equal(s.human.hp, hp - 40);
  assert.equal(s.shots.length, 1);
  s.shots = [{ ...shot(s, -4, 5, 100, 0, 0), life: 0.001 }, shot(s, 4, 5, -100, 0, 1)];
  s.events = [];
  stepProjectiles(s, 0.1);
  assert.equal(s.shots.length, 1);
  assert.equal(s.events.filter((e) => e.type === "explosion").length, 0);
  s.dispose();
});

test("interception blast hurts both teams once and credits the opposing shooter", () => {
  const s = arena(2);
  for (const [i, t] of s.tanks.entries()) {
    t.body.setTranslation({ x: 0, y: 0.65, z: i ? 1.8 : -1.8 }, true);
    t.hp = 40;
  }
  s.world.step();
  s.shots = [shot(s, -3, 0, 100, 0, 0), shot(s, 3, 0, -100, 0, 1)];
  stepProjectiles(s, 0.05);
  assert.ok(s.tanks.every((t) => !t.alive));
  assert.ok(s.tanks.every((t) => t.kills === 1));
  assert.deepEqual(s.match.scores, [1, 1]);
  s.dispose();
});

test("rapid fire modifies only selected ammunition and expires independently", () => {
  const s = arena(1), t = s.human;
  pickup(s, "spread"); pickup(s, "rapid"); pickup(s, "ricochet");
  t.selectedAmmo = "spread";
  fireWeapon(s, t);
  assert.equal(s.shots.length, 3);
  assert.equal(t.cooldown, WEAPONS.spread.interval / 2 / 1.2);
  assert.ok(s.shots.every(p => p.damage === 27 && p.bounces === 1));
  const cooldown = t.cooldown;
  pickup(s, "rapid"); pickup(s, "ricochet");
  assert.equal(t.cooldown, cooldown);
  assert.equal(t.rapid, 12); assert.equal(t.ammo.ricochet, 48);
  t.rapid = STEP; t.cooldown = 0;
  s.step();
  assert.equal(t.rapid, 0); assert.equal(t.ammo.ricochet, 48);
  fireWeapon(s, t);
  assert.equal(t.cooldown, WEAPONS.spread.interval / 1.2);
  t.hp = 1; pickup(s, "repair");
  assert.equal(t.hp, VEHICLES[t.kind].health);
  s.dispose();
});

test("shield absorbs three shells, spills excess damage, expires and resets on respawn", () => {
  const s = arena(1), t = s.human;
  pickup(s, "shield");
  const hp = t.hp;
  for (let i = 0; i < 3; i++) s.damageTank(t, 40, 999, 1);
  assert.equal(t.hp, hp); assert.equal(t.shield, 0); assert.equal(t.shieldPoints, 0);
  s.damageTank(t, 40, 999, 1); assert.equal(t.hp, hp - 40);
  pickup(s, "shield");
  s.damageTank(t, 130, 999, 1); assert.equal(t.hp, hp - 50);
  pickup(s, "shield"); t.shield = STEP; s.step(); assert.equal(t.shieldPoints, 0);
  pickup(s, "rapid"); pickup(s, "ricochet"); pickup(s, "shield");
  s.respawn(t);
  assert.deepEqual([t.rapid, t.ammo.ricochet, t.shield, t.shieldPoints], [0, 0, 0, 0]);
  s.dispose();
});

test("movement adds 20% to V-Tanks-scaled speeds, preserves class ratios and normalizes diagonals", () => {
  assert.ok(Math.abs(VEHICLES.balanced.speed / WEAPONS.standard.speed - 184 / 535 * 1.2) < 1e-9);
  assert.ok(Math.abs(VEHICLES.scout.speed / VEHICLES.balanced.speed - 1.24) < 1e-9);
  assert.ok(Math.abs(VEHICLES.heavy.speed / VEHICLES.balanced.speed - 0.76) < 1e-9);
  const distances: number[] = [];
  for (const diagonal of [false, true]) {
    const s = arena(1), t = s.human;
    t.heading = diagonal ? Math.PI / 4 : 0; // Compare travel after alignment.
    for (let i = 0; i < 60; i++) s.step({ ...idleCommand(), moveX: diagonal ? 1 : 0, moveZ: 1 });
    const p = t.body.translation(); distances.push(Math.hypot(p.x, p.z));
    assert.ok(Math.hypot(t.body.linvel().x, t.body.linvel().z) > VEHICLES[t.kind].speed * 0.98);
    const brakingSteps = Math.ceil(VEHICLES[t.kind].speed / (MOVE_ACCELERATION * STEP));
    for (let i = 0; i < brakingSteps; i++) s.step();
    assert.ok(Math.hypot(t.body.linvel().x, t.body.linvel().z) < 0.01);
    pickup(s, "speed");
    const boostSteps = Math.ceil(VEHICLES[t.kind].speed * 1.5 / (MOVE_ACCELERATION * STEP)) + 2;
    for (let i = 0; i < boostSteps; i++) s.step({ ...idleCommand(), moveX: diagonal ? 1 : 0, moveZ: 1 });
    assert.ok(Math.abs(Math.hypot(t.body.linvel().x, t.body.linvel().z) / VEHICLES[t.kind].speed - 1.5) < 0.03);
    s.dispose();
  }
  assert.ok(Math.abs(distances[0] - distances[1]) < 0.02);
});

test("tracks are distance-spaced at 30/120 FPS, skip stationary tanks and teleports, and stay bounded across resets", () => {
  const counts: number[] = [];
  for (const fps of [30, 120]) {
    const s = arena(1), t = s.human, tracks = new TrackTrails();
    for (let i = 0; i <= fps * 2; i++) {
      const x = i * 6 / fps;
      t.body.setTranslation({ x, y: 0.65, z: 0 }, true); t.previous = { x, z: 0 };
      s.elapsed = i / fps; tracks.update(s, 1);
    }
    counts.push(tracks.mesh.count);
    const before = tracks.mesh.count;
    for (let i = 0; i < 10; i++) tracks.update(s, 1);
    assert.equal(tracks.mesh.count, before);
    t.body.setTranslation({ x: 50, y: 0.65, z: 0 }, true); tracks.update(s, 1);
    assert.equal(tracks.mesh.count, before);
    for (let i = 0; i < 3000; i++) {
      t.body.setTranslation({ x: 50 + i, y: 0.65, z: 0 }, true); tracks.update(s, 1);
    }
    assert.equal(tracks.mesh.count, TRACK_CAPACITY);
    tracks.reset(); assert.equal(tracks.mesh.count, 0);
    tracks.update(s, 1); assert.equal(tracks.mesh.count, 0);
    tracks.dispose(); s.dispose();
  }
  assert.equal(counts[0], counts[1]);
});

for (const order of [["spread", "rocket"], ["rocket", "spread"]] as const) {
  test(`${order.join(" then ")} supplies independent ammo without selecting or combining it`, () => {
    const s = arena(1), t = s.human;
    for (const kind of order) pickup(s, kind);
    pickup(s, "rapid"); pickup(s, "ricochet");
    assert.equal(t.selectedAmmo, "standard");
    for (const weapon of ["standard", "spread", "rocket", "ricochet"] as const) {
      t.selectedAmmo = weapon; t.cooldown = 0; s.shots = [];
      fireWeapon(s, t);
      assert.equal(s.shots.length, weapon === "spread" ? 3 : 1);
      assert.ok(s.shots.every(p => p.weapon === weapon && p.damage === WEAPONS[weapon].damage));
      assert.equal(t.cooldown, WEAPONS[weapon].interval / 2 / 1.2);
    }
    s.dispose();
  });
}

test("four repair pickups are symmetric, clear of cover, and away from spawn pads", () => {
  const s = new Simulation(123);
  const repairs = s.pickups.filter(p => p.kind === "repair");
  assert.equal(repairs.length, 4);
  for (const p of repairs) {
    assert.ok(repairs.some(other => other.x === -p.x && other.z === -p.z));
    assert.ok(s.covers.every(c => Math.abs(p.x - c.x) > c.w / 2 + 2
      || Math.abs(p.z - c.z) > c.d / 2 + 2));
    for (const spawn of [...spawnPositions(0), ...spawnPositions(1)])
      assert.ok(Math.hypot(p.x - spawn.x, p.z - spawn.z) >= 8,
        `Repair pickup at ${p.x},${p.z} is too close to a spawn pad`);
  }
  s.dispose();
});
