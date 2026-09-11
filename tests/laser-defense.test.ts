import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { collectPickup, stepProjectiles } from "../src/game/weapons";
import { LASER_DEFENSE, Random, STEP } from "../src/game/data";
import { botCommand } from "../src/game/ai";
import { idleCommand, type Shot, type Weapon, type Pickup } from "../src/game/types";
before(async () => {
  await RAPIER.init();
});
function fixture() {
  const s = new Simulation(123),
    t = s.human;
  for (const bot of s.tanks) if (bot !== t) s.world.removeRigidBody(bot.body);
  s.tanks = [t];
  for (const cover of s.covers) s.world.removeRigidBody(cover.body);
  s.covers = [];
  s.coverByCollider.clear();
  s.nav.rebuild([]);
  s.pickups = [];
  t.team = s.humanTeam = 0;
  t.protection = 0;
  t.laser = LASER_DEFENSE.duration;
  t.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
  t.body.setRotation({ x: 0, y: 0, z: 0, w: 1 }, true);
  t.previous = { x: 0, z: 0 };
  s.world.step();
  s.start();
  s.events = [];
  return { s, t };
}
function incoming(s: Simulation, weapon: Weapon = "standard", override: Partial<Shot> = {}) {
  const p: Shot = {
    id: s.nextId++,
    x: 0,
    z: -6,
    vx: 0,
    vz: 20,
    team: 1,
    owner: 999,
    weapon,
    damage: 40,
    bounces: 0,
    life: 3.5,
    piercing: weapon === "piercing" ? 1 : 0,
    ...override,
  };
  s.shots.push(p);
  return p;
}
function pickup(s: Simulation): Pickup {
  return { id: s.nextId++, kind: "laser", x: 0, z: 0, available: true, cooldown: 0 };
}

test("laser stops every munition without splash, kill credit or changing cannon cooldown", () => {
  for (const weapon of ["standard", "spread", "rocket", "ricochet", "piercing"] as const) {
    const { s, t } = fixture();
    s.rng.next = () => 0;
    t.cooldown = 0.6;
    const hp = t.hp;
    incoming(s, weapon);
    stepProjectiles(s, STEP);
    assert.equal(s.shots.length, 0, weapon);
    assert.equal(t.hp, hp);
    assert.equal(t.cooldown, 0.6);
    assert.deepEqual(s.match.scores, [0, 0]);
    assert.equal(t.kills, 0);
    assert.deepEqual(
      s.events.map((e) => e.type),
      ["laser"],
    );
    assert.equal(s.events[0].id, t.id);
    assert.equal(s.events[0].from!.x, 0);
    assert.ok(s.events[0].from!.y > 1);
    s.dispose();
  }
});

test("50 percent chance is rolled once per shot and tank; misses can still hit", () => {
  const { s, t } = fixture();
  let rolls = 0;
  s.rng.next = () => {
    rolls++;
    return 0.5;
  };
  const p = incoming(s);
  const hp = t.hp;
  for (let i = 0; i < 20 && s.shots.length; i++) stepProjectiles(s, STEP);
  assert.equal(rolls, 1);
  assert.deepEqual(p.laserCheckedBy, [t.id]);
  assert.equal(t.hp, hp - 40);
  assert.equal(s.events.filter((e) => e.type === "laser").length, 0);
  s.dispose();
});

test("seeded sampling stays near 50 percent instead of becoming guaranteed over many frames", () => {
  const { s } = fixture();
  s.rng = new Random(7788);
  let blocked = 0;
  for (let i = 0; i < 1000; i++) {
    incoming(s);
    stepProjectiles(s, STEP);
    if (!s.shots.length) blocked++;
    s.shots = [];
    s.events = [];
  }
  assert.ok(blocked > 450 && blocked < 550, `${blocked}/1000`);
  s.dispose();
});

test("laser ignores allies, outgoing and harmless passing shots, inactive and dead defenders", () => {
  for (const mode of ["ally", "outgoing", "passing", "out-of-range", "expired", "dead"]) {
    const { s, t } = fixture();
    let rolls = 0;
    s.rng.next = () => {
      rolls++;
      return 0;
    };
    if (mode === "expired") t.laser = 0;
    if (mode === "dead") {
      s.damageTank(t, 999, t.id, t.team);
      assert.equal(t.laser, 0);
    }
    rolls = 0; // Wreck construction consumes the same seeded RNG during setup.
    incoming(
      s,
      "standard",
      mode === "ally"
        ? { team: 0 }
        : mode === "outgoing"
          ? { vz: -20 }
          : mode === "passing"
            ? { x: 5 }
            : mode === "out-of-range"
              ? { z: -12 }
              : {},
    );
    stepProjectiles(s, STEP);
    assert.equal(rolls, 0, mode);
    assert.equal(s.shots.length, 1, mode);
    s.dispose();
  }
});

test("swept range entry catches fast shells before impact; earlier cover still wins", () => {
  for (const wall of [false, true]) {
    const { s, t } = fixture();
    let rolls = 0;
    s.rng.next = () => {
      rolls++;
      return 0;
    };
    if (wall)
      s.addCover({ kind: "concrete", x: 0, z: -9, w: 5, d: 0.4, h: 3, hp: Infinity, color: 0 });
    s.world.step();
    incoming(s, "standard", { z: -12, vz: 1200 });
    stepProjectiles(s, STEP);
    assert.equal(rolls, wall ? 0 : 1);
    assert.equal(s.shots.length, 0);
    assert.equal(t.hp, 100);
    const beam = s.events.find((e) => e.type === "laser");
    assert.equal(!!beam, !wall);
    if (beam) assert.ok(Math.abs(beam.z + LASER_DEFENSE.range) < 1e-6);
    s.dispose();
  }
});

test("cover occludes lasers within range; shells already hitting the hull take priority", () => {
  for (const mode of ["cover", "hull"]) {
    const { s, t } = fixture();
    let rolls = 0;
    s.rng.next = () => {
      rolls++;
      return 0;
    };
    if (mode === "cover")
      s.addCover({ kind: "concrete", x: 0, z: -3, w: 5, d: 0.5, h: 3, hp: Infinity, color: 0 });
    s.world.step();
    incoming(s, "standard", mode === "hull" ? { z: -0.5 } : {});
    stepProjectiles(s, STEP);
    assert.equal(rolls, 0, mode);
    assert.equal(t.hp, mode === "hull" ? 60 : 100);
    s.dispose();
  }
});

test("moving defenders intercept at their swept position", () => {
  const { s, t } = fixture();
  s.rng.next = () => 0;
  t.body.setTranslation({ x: 0, y: 0.65, z: -1 }, true);
  t.previous = { x: 0, z: 0 };
  incoming(s, "standard", { z: -7.5 });
  stepProjectiles(s, STEP, true);
  assert.equal(s.shots.length, 0);
  const e = s.events.find((e) => e.type === "laser")!;
  assert.ok(e.from!.z < 0 && e.from!.z > -1);
  assert.ok(Math.abs(e.z - e.from!.z + LASER_DEFENSE.range) < 1e-6);
  s.dispose();
});

test("multiple missed defenses do not exhaust the contact budget or freeze projectile time", () => {
  const { s, t } = fixture();
  for (let i = 0; i < 24; i++) {
    const friend = s.addTank(0, false, "balanced", i);
    friend.laser = 6;
    friend.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
  }
  s.rng.next = () => 0.8;
  const p = incoming(s);
  stepProjectiles(s, STEP);
  assert.equal(p.laserCheckedBy!.length, 25);
  assert.ok(Math.abs(p.z - (-6 + 20 * STEP)) < 1e-7);
  assert.equal(t.hp, 100);
  s.dispose();
});

test("laser refreshes to twenty seconds, pauses, expires, and clears on death/respawn/reset", () => {
  const { s, t } = fixture();
  const p = pickup(s);
  t.laser = 2;
  t.cooldown = 0.4;
  assert.equal(collectPickup(s, t, p), true);
  assert.equal(t.laser, 20);
  assert.equal(p.cooldown, 45);
  assert.equal(p.cooldownDuration, 45);
  assert.equal(t.cooldown, 0.4);
  assert.equal(collectPickup(s, t, p), false);
  s.match.phase = "paused";
  for (let i = 0; i < 60; i++) s.step();
  assert.equal(t.laser, 20);
  s.start();
  for (let i = 0; i < 1201; i++) s.step(idleCommand());
  assert.equal(t.laser, 0);
  collectPickup(s, t, pickup(s));
  s.damageTank(t, 999, t.id, t.team);
  assert.equal(t.laser, 0);
  s.respawn(t);
  assert.equal(t.laser, 0);
  collectPickup(s, t, pickup(s));
  s.reset();
  assert.ok(s.tanks.every((t) => t.laser === 0));
  assert.ok(s.snapshot().tanks.every((t) => t.laser === 0));
  s.dispose();
});

test("one central rare pickup starts delayed and refills much slower than ordinary pickups", () => {
  const s = new Simulation(12);
  const rare = s.pickups.filter((p) => p.kind === "laser");
  assert.equal(rare.length, 1);
  assert.deepEqual([rare[0].x, rare[0].z], [0, 0]);
  assert.equal(rare[0].available, false);
  assert.equal(rare[0].cooldown, 25);
  assert.equal(rare[0].cooldownDuration, 25);
  assert.ok(s.pickups.filter((p) => p.kind !== "laser").every((p) => p.available));
  s.dispose();
  const { s: arena, t } = fixture();
  const p = pickup(arena);
  arena.pickups = [p];
  collectPickup(arena, t, p);
  t.body.setTranslation({ x: 15, y: 0.65, z: 0 }, true);
  t.previous = { x: 15, z: 0 };
  for (let i = 0; i < 44 * 60; i++) arena.step();
  assert.equal(p.available, false);
  for (let i = 0; i < 61; i++) arena.step();
  assert.equal(p.available, true);
  arena.dispose();
});

test("bots seek an available laser when useful and leave it while theirs is fresh", () => {
  const { s, t } = fixture();
  t.human = false;
  t.laser = 0;
  s.pickups = [{ ...pickup(s), z: 4 }];
  botCommand(s, t, STEP);
  assert.equal(t.brain.pickupTarget, s.pickups[0].id);
  t.laser = 6;
  t.brain.decision = 0;
  botCommand(s, t, STEP);
  assert.equal(t.brain.pickupTarget, 0);
  s.dispose();
});
