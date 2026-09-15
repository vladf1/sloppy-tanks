import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { blastDebris } from "../src/game/debris-physics";
import { GROUP, Random, STEP } from "../src/game/data";
import { Simulation } from "../src/game/simulation";
import { stepProjectiles } from "../src/game/projectiles";
import type { CoverKind, Shot } from "../src/game/types";

before(async () => {
  await RAPIER.init();
});
function arena() {
  const s = new Simulation(731);
  for (const c of s.covers) s.world.removeRigidBody(c.body);
  for (const t of s.tanks) s.world.removeRigidBody(t.body);
  s.covers = [];
  s.movableCovers = [];
  s.coverByCollider.clear();
  s.tanks = [];
  s.pickups = [];
  s.nav.rebuild([]);
  s.start();
  return s;
}
function cover(s: Simulation, kind: CoverKind, x = 0, z = 0) {
  const c = s.addCover({
    kind,
    x,
    z,
    w: 2,
    h: kind === "tree" ? 6 : 2,
    d: 2,
    hp: kind === "teeth" || kind === "hedgehog" ? Infinity : 40,
    color: 0x92734e,
  });
  s.nav.rebuild(s.covers);
  s.world.step();
  return c;
}
function shot(s: Simulation, weapon: Shot["weapon"] = "standard") {
  s.shots.push({
    id: s.nextId++,
    owner: 999,
    team: 0,
    x: -4,
    z: 0,
    vx: 25,
    vz: 0,
    damage: 40,
    bounces: 0,
    life: 2,
    weapon,
    piercing: 0,
  });
  stepProjectiles(s, 0.2);
}
function tick(s: Simulation, seconds: number) {
  for (let i = 0; i < seconds / STEP; i++) s.step();
}
function wreck(s: Simulation) {
  const tank = s.addTank(1, false, "balanced", 0);
  tank.protection = 0;
  tank.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
  s.damageTank(tank, 1000, 999, 0);
  s.tanks = [];
  return s.fragments.find((f) => f.part?.startsWith("turret"))!;
}
function park(body: RAPIER.RigidBody, x: number, y: number, z = 0) {
  body.setTranslation({ x, y, z }, true);
  body.setLinvel({ x: 0, y: 0, z: 0 }, true);
  body.setAngvel({ x: 0, y: 0, z: 0 }, true);
  body.sleep();
}

test("blasts wake and tumble a wreck; edge, distant and airborne debris obey falloff without RNG draws", () => {
  const s = arena();
  try {
    const f = wreck(s);
    const state = s.rng.state;
    park(f.body, 1, 0.5);
    blastDebris(s, { x: 0, z: 0 }, 5, 60);
    const near = f.body.linvel().y;
    assert.ok(near > 8);
    assert.ok(Math.hypot(...Object.values(f.body.angvel())) > 0.1);
    assert.equal(f.body.isSleeping(), false);
    park(f.body, 4.8, 0.5);
    blastDebris(s, { x: 0, z: 0 }, 5, 60);
    assert.ok(f.body.linvel().y < near * 0.02);
    for (const [x, y] of [
      [6, 0.5],
      [0, 8],
    ]) {
      park(f.body, x, y);
      blastDebris(s, { x: 0, z: 0 }, 5, 60);
      assert.equal(f.body.linvel().y, 0);
      assert.equal(f.body.isSleeping(), true);
    }
    assert.equal(s.rng.state, state);
  } finally {
    s.dispose();
  }
});

test("a turret lands, then a drum chain naturally launches that same body again", () => {
  const s = arena();
  try {
    const f = wreck(s);
    const id = f.body.handle;
    tick(s, 4);
    assert.ok(f.body.translation().y < 1.5);
    const p = f.body.translation();
    const drum = cover(s, "drum", p.x + 1.4, p.z);
    cover(s, "drum", p.x + 3.5, p.z);
    s.damageCover(drum, 100, 999, 0);
    assert.ok(f.body.linvel().y > 4);
    assert.equal(f.body.handle, id);
    assert.ok(s.events.filter((e) => e.type === "explosion").length >= 2);
    tick(s, 0.3);
    assert.ok(f.body.translation().y > p.y + 0.5);
  } finally {
    s.dispose();
  }
});

test("real projectile hits shove concrete cumulatively, while rockets and nearby blasts are stronger", () => {
  const speeds: number[] = [];
  for (const mode of ["standard", "rocket", "blast"] as const) {
    const s = arena();
    try {
      const c = cover(s, "teeth");
      if (mode === "blast") s.explode({ x: -1, z: 0 }, 5, 80, 999, 0);
      else shot(s, mode);
      const v = c.body.linvel();
      speeds.push(Math.hypot(v.x, v.y, v.z));
      assert.ok(v.x > 0.5);
      assert.equal(c.alive, true);
      assert.equal(c.hp, Infinity);
      assert.ok(Math.abs(c.body.mass() - 6.9984) < 0.001);
      assert.ok(c.collider.friction() >= 1);
      assert.ok(c.collider.restitution() < 0.05);
      if (mode === "standard") {
        shot(s);
        assert.ok(c.body.linvel().x > v.x * 1.8);
        assert.ok(Math.abs(c.body.angvel().z) > 0.1);
      }
    } finally {
      s.dispose();
    }
  }
  assert.ok(speeds[1] > speeds[0] * 2);
  assert.ok(speeds[2] > speeds[0] * 2);
});

test("repeated impacts displace concrete, update old/new navigation footprints and eventually sleep", () => {
  const s = arena();
  try {
    const c = cover(s, "teeth");
    const initialVersion = s.nav.version;
    s.explode({ x: -1, z: 0 }, 6, 100, 999, 0);
    tick(s, 8);
    assert.ok(c.x > 3, `displacement ${c.x}`);
    assert.equal(s.nav.blocked[s.nav.index({ x: 0, z: 0 })], 0);
    assert.equal(s.nav.blocked[s.nav.index(c)], 1);
    assert.ok(s.nav.version > initialVersion);
    assert.ok(s.nav.version - initialVersion <= 32, "no per-frame rebuilds");
    assert.ok(c.body.isSleeping(), "settled heavy concrete should sleep");
    assert.ok(c.body.translation().y > 0, "concrete collides with ground");
    const version = s.nav.version;
    tick(s, 2);
    assert.equal(s.nav.version, version);
    const path = s.nav.find({ x: c.x - 8, z: c.z }, { x: c.x + 8, z: c.z });
    assert.ok(path.length > 0);
    assert.ok(path.every((p) => !s.nav.blocked[s.nav.index(p)]));
  } finally {
    s.dispose();
  }
});

test("authored scenery emits a few material-specific pieces with matching dimensions and contact telemetry", () => {
  for (const [kind, shapes] of [
    ["cargo", ["panel", "panel", "panel", "beam"]],
    ["timber", ["beam", "beam", "beam"]],
    ["tree", ["log", "beam"]],
    ["drum", ["drum-shell", "drum-lid"]],
    ["tower", ["panel", "beam", "panel", "beam"]],
  ] as const) {
    const s = arena();
    try {
      const c = cover(s, kind);
      const expectedRng = new Random(s.rng.state);
      const oldDraws = { cargo: 33, timber: 77, tree: 99, drum: 33, tower: 112 };
      for (let i = 0; i < oldDraws[kind]; i++) expectedRng.next();
      s.damageCover(c, 1000, 999, 0);
      assert.equal(
        s.rng.state,
        expectedRng.state,
        "destruction preserves the legacy combat RNG stream",
      );
      assert.deepEqual(
        s.fragments.map((f) => f.shape),
        shapes,
      );
      for (const f of s.fragments) {
        assert.equal(f.sourceKind, kind);
        assert.ok(f.dimensions);
        assert.ok(f.body.isDynamic());
        assert.equal(f.body.collider(0).collisionGroups(), GROUP.fragment);
        assert.equal(f.body.isCcdEnabled(), false);
      }
      tick(s, 4);
      assert.ok(s.events.some((e) => e.type === "debris-impact" && e.material && e.force! > 0));
      tick(s, 15);
      assert.equal(s.fragments.length, 0);
    } finally {
      s.dispose();
    }
  }
});

test("physical pieces stay within the shared body budget, cannot hit tanks or intercept shells, and reset cleanly", () => {
  const s = arena();
  try {
    const initial = s.world.bodies.len();
    for (let i = 0; i < 35; i++) {
      const c = cover(s, "cargo", (i % 7) * 5 - 15, Math.floor(i / 7) * 5 - 10);
      s.damageCover(c, 100, 999, 0);
    }
    assert.equal(s.fragments.length, s.maxFragments);
    assert.equal(s.world.bodies.len(), initial + s.maxFragments);
    const allows = (a: number, b: number) =>
      ((a >>> 16) & b & 0xffff) !== 0 && ((b >>> 16) & a & 0xffff) !== 0;
    assert.equal(allows(GROUP.fragment, GROUP.tank), false);
    assert.equal(allows(GROUP.fragment, GROUP.fragment), false);
    assert.equal(allows(GROUP.fragment, GROUP.ground), true);
    assert.equal(allows(GROUP.fragment, GROUP.movableCover), true);
    s.world.step();
    assert.equal(
      s.world.castRay(
        new RAPIER.Ray({ x: -20, y: 1, z: 0 }, { x: 1, y: 0, z: 0 }),
        40,
        true,
        undefined,
        GROUP.coverQuery,
      ),
      null,
    );
    tick(s, 19);
    assert.equal(s.fragments.length, 0);
    assert.equal(s.world.bodies.len(), initial);
    s.mapMode = "quarry";
    s.reset();
    const bodies = s.world.bodies.len();
    const colliders = s.world.colliders.len();
    s.start();
    s.explode(s.movableCovers[0], 6, 100, 999, 0);
    tick(s, 1);
    s.reset();
    assert.equal(s.world.bodies.len(), bodies);
    assert.equal(s.world.colliders.len(), colliders);
    assert.equal(s.fragments.length, 0);
    assert.equal(s.movableCovers.length, 24);
    assert.ok(s.movableCovers.every((c) => c.body.isValid()));
  } finally {
    s.dispose();
  }
});

test("physical destruction and blast replay remain deterministic for a fixed seed", () => {
  const run = () => {
    const s = arena();
    try {
      cover(s, "teeth", 4, 0);
      s.damageCover(cover(s, "cargo"), 100, 999, 0);
      for (let i = 0; i < 180; i++) {
        if (i % 60 === 0) s.explode({ x: 1, z: 0 }, 6, 80, 999, 0);
        s.step();
      }
      return [...s.fragments, ...s.movableCovers].map((f) => ({
        p: f.body.translation(),
        q: f.body.rotation(),
        v: f.body.linvel(),
      }));
    } finally {
      s.dispose();
    }
  };
  assert.deepEqual(run(), run());
});

test("bots route around a displaced tooth and cross its former position without repeated recovery", () => {
  const s = arena();
  try {
    const c = cover(s, "teeth");
    s.explode({ x: -1, z: 0 }, 6, 100, 999, 0);
    tick(s, 8);
    const human = s.addTank(0, true, "balanced", 0);
    human.body.setTranslation({ x: 40, y: 0.65, z: 40 }, true);
    const bot = s.addTank(0, false, "balanced", 1);
    const from = { x: c.x - 8, z: c.z };
    const goal = { x: c.x + 8, z: c.z };
    bot.body.setTranslation({ ...from, y: 0.65 }, true);
    bot.previous = { ...from };
    bot.brain.last = { ...from };
    bot.brain.decision = 999;
    bot.brain.goal = goal;
    bot.brain.path = s.nav.find(from, goal);
    bot.brain.navVersion = s.nav.version;
    tick(s, 12);
    const p = bot.body.translation();
    assert.ok(Math.hypot(p.x - goal.x, p.z - goal.z) < 1, `bot stopped at ${p.x}/${p.z}`);
    assert.ok(bot.brain.recoveries <= 2, "bot does not keep driving into the moved barrier");
  } finally {
    s.dispose();
  }
});

test("steel hedgehogs keep open compound geometry and move, settle and update navigation after blasts", () => {
  const s = arena();
  try {
    const c = cover(s, "hedgehog");
    assert.equal(c.body.numColliders(), 9);
    assert.ok(Math.abs(c.body.mass() - 6) < 0.001);
    for (let i = 0; i < c.body.numColliders(); i++) {
      assert.equal(s.coverByCollider.get(c.body.collider(i).handle), c);
    }
    s.explode({ x: -1, z: 0 }, 5, 80, 999, 0);
    assert.ok(c.body.linvel().x > 3);
    assert.ok(Math.hypot(...Object.values(c.body.angvel())) > 1);
    tick(s, 10);
    assert.ok(c.x > 3);
    assert.ok(c.body.translation().y > 0);
    assert.ok(c.body.isSleeping());
    assert.equal(s.nav.blocked[s.nav.index(c)], 1);
    assert.equal(s.nav.blocked[s.nav.index({ x: 0, z: 0 })], 0);
    assert.ok(s.events.some((e) => e.type === "debris-impact" && e.material === "metal"));
  } finally {
    s.dispose();
  }
});

test("quarry dragon teeth are 10 percent smaller in every dimension with density-preserving mass", () => {
  const s = new Simulation(731);
  try {
    s.mapMode = "quarry";
    s.reset();
    const c = s.covers.find((item) => item.kind === "teeth" && item.x === 41.7)!;
    assert.ok(Math.abs(c.w - 1.71) < 0.001);
    assert.ok(Math.abs(c.d - 1.71) < 0.001);
    assert.ok(Math.abs(c.h - 1.71) < 0.001);
    assert.ok(Math.abs(c.body.mass() - 9.6 * 0.9 ** 3) < 0.001);
  } finally {
    s.dispose();
  }
});
