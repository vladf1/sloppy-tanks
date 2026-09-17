import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { blastDebris } from "../src/game/debris-physics";
import { DEBRIS_CLEANUP_SECONDS } from "../src/game/debris-cleanup";
import { GROUP, Random, STEP } from "../src/game/data";
import { Simulation } from "../src/game/simulation";
import { stepProjectiles } from "../src/game/projectiles";
import type { CoverKind, Shot } from "../src/game/types";
import { idleCommand } from "../src/game/types";
import { treeProportions } from "../src/game/tree-proportions";

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
function shot(s: Simulation, weapon: Shot["weapon"] = "standard", y = 1) {
  s.shots.push({
    id: s.nextId++,
    owner: 999,
    team: 0,
    x: -4,
    z: 0,
    y,
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

test("rooted stumps block every chassis after debris cleanup and leave the crown space open", () => {
  for (const kind of ["scout", "balanced", "heavy"] as const) {
    for (const heading of [0, Math.PI / 2, Math.PI, -Math.PI / 2]) {
      const s = arena();
      try {
        const tree = cover(s, "tree");
        s.damageCover(tree, 1000, 999, 0);
        // Isolate the permanent stump from the temporary falling log.
        for (const fragment of s.fragments) s.world.removeRigidBody(fragment.body);
        s.fragments = [];
        const tank = s.addTank(0, true, kind);
        const direction = { x: Math.sin(heading), z: Math.cos(heading) };
        tank.heading = heading;
        tank.body.setRotation(
          { x: 0, y: Math.sin(heading / 2), z: 0, w: Math.cos(heading / 2) },
          true,
        );
        tank.body.setTranslation({ x: -5 * direction.x, y: 0.65, z: -5 * direction.z }, true);
        tank.previous = { x: -5 * direction.x, z: -5 * direction.z };
        s.world.step();
        for (let i = 0; i < 180; i++) {
          s.step({ ...idleCommand(), moveX: direction.x, moveZ: direction.z });
        }
        const position = tank.body.translation();
        assert.ok(
          position.x * direction.x + position.z * direction.z < -0.7,
          `${kind} must stop before the stump`,
        );
        assert.ok(position.y < 0.8, "the stump must not lift the tank over its footprint");
        assert.equal(tree.body.isValid(), true);
        assert.equal(tree.body.isFixed(), true);
        assert.equal(s.nav.blocked[s.nav.index(tree)], 1, "bots must route around the stump");
        assert.equal(s.nav.clearLine({ x: -4, z: 0 }, { x: 4, z: 0 }), false);
        assert.equal(
          s.world.castRay(
            new RAPIER.Ray({ x: -4, y: 1, z: 0 }, { x: 1, y: 0, z: 0 }),
            8,
            true,
            undefined,
            GROUP.coverQuery,
          ),
          null,
          "shells must fly above the stump",
        );
        const ray = new RAPIER.Ray({ x: -4, y: 0.65, z: 0 }, { x: 1, y: 0, z: 0 });
        assert.ok(
          s.world.castRay(ray, 8, true, undefined, GROUP.steeringQuery, undefined, tank.body),
          "local steering must detect the stump",
        );
        assert.ok(Math.abs(tree.collider.radius() - treeProportions(tree).stumpRadius) < 1e-6);
        const destroyed = s.destroyed;
        s.damageCover(tree, 1000, 999, 0);
        assert.equal(s.destroyed, destroyed, "a stump cannot be destroyed twice");
      } finally {
        s.dispose();
      }
    }
  }
});

test("destroyed trees leave a narrow stump rather than the original canopy-sized obstacle", () => {
  const s = arena();
  try {
    const tree = cover(s, "tree");
    assert.equal(s.nav.blocked[s.nav.index({ x: 2, z: 0 })], 1);
    s.damageCover(tree, 1000, 999, 0);
    for (const fragment of s.fragments) s.world.removeRigidBody(fragment.body);
    s.fragments = [];
    assert.equal(s.nav.blocked[s.nav.index({ x: 2, z: 0 })], 0);
    const tank = s.addTank(0, true, "heavy");
    tank.heading = 0;
    park(tank.body, 2, 0.65, -5);
    for (let i = 0; i < 120; i++) s.step({ ...idleCommand(), moveZ: 1 });
    assert.ok(tank.body.translation().z > 2, "a tank can pass beside the solid stump");
    s.reset();
    assert.ok(
      s.covers
        .filter((c) => c.kind === "tree")
        .every((c) => c.alive && c.collider.collisionGroups() === GROUP.cover),
    );
  } finally {
    s.dispose();
  }
});

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
      assert.ok(c.collider.friction() >= 0.6);
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
    ["drum", ["drum-shell", "drum-shell", "drum-shell", "drum-lid"]],
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
        assert.equal(f.body.collider(0).collisionGroups(), GROUP.pushableDebris);
        assert.equal(f.body.isCcdEnabled(), false);
      }
      if (kind === "tree") {
        const trunk = s.fragments.find((f) => f.shape === "log")!;
        assert.equal(trunk.treeCoverId, c.id, "the falling model retains its source crown");
        assert.ok(Math.abs(trunk.body.translation().y - trunk.treeCenterY!) < 1e-5);
        assert.equal(trunk.body.linvel().y, 0, "a severed tree falls rather than launching upward");
        assert.equal(trunk.body.numColliders(), 2, "both trunk and crown contact the ground");
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

test("physical pieces stay within the shared body budget, cannot intercept shells, and reset cleanly", () => {
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

test("destroying finite movable cover removes its body without poisoning later simulation steps", () => {
  const s = arena();
  try {
    const movable = cover(s, "teeth");
    movable.hp = movable.maxHp = 40;
    movable.destructible = true;

    s.damageCover(movable, 40, 999, 0);

    assert.equal(movable.alive, false);
    assert.equal(movable.body.isValid(), false);
    assert.doesNotThrow(() => s.step());
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

test("a scout can steadily push every concrete profile, with throttled navigation and no damage", () => {
  // x = 0..3 selects all four authored profiles, using the tallest quarry tooth.
  for (const x of [0, 1, 2, 3]) {
    const s = arena();
    try {
      const c = s.addCover({
        kind: "teeth",
        x,
        z: 0,
        w: 1.845,
        h: 1.845,
        d: 1.845,
        hp: Infinity,
        color: 0xaaaaaa,
      });
      s.nav.rebuild(s.covers);
      const version = s.nav.version;
      const tank = s.addTank(0, true, "scout");
      tank.heading = Math.PI / 2;
      park(tank.body, x - 2.8, 0.65);
      const hp = tank.hp;
      for (let i = 0; i < 240; i++) s.step({ ...idleCommand(), moveX: 1 });
      assert.ok(c.x - x > 1, `profile ${x} moved only ${c.x - x}`);
      assert.ok(tank.body.translation().x > x - 1.8);
      assert.ok(tank.body.translation().y < 0.8, "tank does not climb the concrete");
      assert.equal(tank.hp, hp);
      assert.equal(c.hp, Infinity);
      assert.ok(s.nav.version > version && s.nav.version - version <= 16);
      assert.equal(s.nav.blocked[s.nav.index(c)], 1);
      s.world.removeRigidBody(tank.body);
      s.tanks = [];
      tick(s, 8);
      assert.ok(c.body.isSleeping(), "concrete settles after pushing stops");
    } finally {
      s.dispose();
    }
  }
});

test("tanks physically shove landed hulls and turrets without damage, and wreck cleanup still removes bodies", () => {
  for (const part of ["hull", "turret"] as const) {
    const s = arena();
    try {
      const turret = wreck(s);
      const f = part === "hull" ? s.fragments.find((f) => f.part === "hull")! : turret;
      for (const other of s.fragments) park(other.body, 30, 1);
      f.body.setRotation({ x: 0, y: 0, z: 0, w: 1 }, true);
      park(f.body, 0, 0.5);
      tick(s, 0.5);
      const tank = s.addTank(0, true, "scout");
      tank.heading = Math.PI / 2;
      park(tank.body, -4, 0.65);
      const hp = tank.hp;
      for (let i = 0; i < 120; i++) s.step({ ...idleCommand(), moveX: 1 });
      assert.ok(f.body.translation().x > 2, `${part} must move through tank contact`);
      assert.ok(tank.body.translation().x > 0, "wreck does not trap the tank");
      assert.ok(tank.body.translation().y < 0.8, "tank stays grounded");
      assert.equal(tank.hp, hp);
      tick(s, 19);
      assert.equal(s.fragments.length, 0);
      assert.equal(f.body.isValid(), false);
      assert.equal(s.world.bodies.len(), 2, "only tank and ground remain");
      wreck(s);
      s.reset();
      assert.equal(s.fragments.length, 0);
      const bodies = s.world.bodies.len();
      s.reset();
      assert.equal(s.world.bodies.len(), bodies, "reset does not retain wreck bodies");
    } finally {
      s.dispose();
    }
  }
});

test("only large wrecks accept tank contact and projectile hits; steering still excludes wrecks", () => {
  const allows = (a: number, b: number) =>
    ((a >>> 16) & b & 0xffff) !== 0 && ((b >>> 16) & a & 0xffff) !== 0;
  assert.equal(allows(GROUP.wreck, GROUP.tank), true);
  assert.equal(allows(GROUP.wreck, GROUP.wreckQuery), true);
  for (const group of [
    GROUP.fragment,
    GROUP.pushableDebris,
    GROUP.coverQuery,
    GROUP.steeringQuery,
  ]) {
    assert.equal(allows(GROUP.wreck, group), false);
  }
  assert.equal(allows(GROUP.fragment, GROUP.tank), false);
  assert.equal(allows(GROUP.wreck, GROUP.ground), true);
  assert.equal(allows(GROUP.wreck, GROUP.movableCover), true);
  const s = arena();
  try {
    wreck(s);
    for (const f of s.fragments) {
      assert.equal(
        f.body.collider(0).collisionGroups(),
        f.part === "barrel" ? GROUP.fragment : GROUP.wreck,
      );
      park(f.body, 0, 1);
    }
    s.world.step();
    for (const group of [GROUP.coverQuery, GROUP.steeringQuery]) {
      assert.equal(
        s.world.castRay(
          new RAPIER.Ray({ x: -5, y: 1, z: 0 }, { x: 1, y: 0, z: 0 }),
          10,
          true,
          undefined,
          group,
        ),
        null,
      );
    }
  } finally {
    s.dispose();
  }
});

test("shells shove indestructible wrecks, rockets detonate on them, and high rounds clear them", () => {
  for (const part of ["hull", "turret"] as const) {
    const s = arena();
    try {
      const turret = wreck(s);
      const f =
        part === "hull" ? s.fragments.find((fragment) => fragment.part === "hull")! : turret;
      for (const other of s.fragments) park(other.body, 30, 1);
      f.body.setRotation({ x: 0, y: 0, z: 0, w: 1 }, true);
      park(f.body, 0, 0.65);
      s.world.step();
      s.events = [];
      shot(s, "standard", 1);
      assert.equal(s.shots.length, 0, `${part} absorbs the shell`);
      assert.ok(f.body.linvel().x > 0, `${part} moves from the impact`);
      assert.equal(
        s.events.some((event) => event.type === "explosion"),
        false,
      );

      park(f.body, 0, 0.65);
      s.world.propagateModifiedBodyPositionsToColliders();
      s.events = [];
      shot(s, "rocket", 1);
      assert.equal(s.shots.length, 0, `rocket impacts the ${part}`);
      assert.equal(
        s.events.some((event) => event.type === "explosion"),
        true,
      );
      assert.ok(s.fragments.includes(f), `${part} survives the rocket blast`);
      assert.equal(f.body.isValid(), true);

      park(f.body, 0, 0.25);
      s.world.propagateModifiedBodyPositionsToColliders();
      s.events = [];
      shot(s, "standard", 1);
      assert.equal(s.shots.length, 1, `shell passes above half-sunken ${part}`);
      assert.ok(s.shots[0].x > 0);
    } finally {
      s.dispose();
    }
  }
});

test("a scout pushes fallen logs, beams, panels and drum pieces while small chips stay nonblocking", () => {
  for (const [kind, shape] of [
    ["tree", "log"],
    ["timber", "beam"],
    ["cargo", "panel"],
    ["drum", "drum-shell"],
    ["drum", "drum-lid"],
  ] as const) {
    const s = arena();
    try {
      // Destroy the source away from the push lane; rooted stumps stay solid.
      const c = cover(s, kind, 20, 20);
      s.damageCover(c, 999, 999, 0);
      const f = s.fragments.find((fragment) => fragment.shape === shape)!;
      for (const other of s.fragments) park(other.body, 30, 1);
      // Lay tall panels and logs flat, exercising ground contact rather than upright props.
      const tipped = shape === "log" || shape === "panel";
      f.body.setRotation(
        { x: tipped ? Math.SQRT1_2 : 0, y: 0, z: 0, w: tipped ? Math.SQRT1_2 : 1 },
        true,
      );
      park(f.body, 0, 2);
      f.body.wakeUp();
      tick(s, 1.5);
      const startX = f.body.translation().x;
      const tank = s.addTank(0, true, "scout");
      tank.heading = Math.PI / 2;
      park(tank.body, -4, 0.65);
      const hp = tank.hp;
      for (let i = 0; i < 120; i++) s.step({ ...idleCommand(), moveX: 1 });
      assert.ok(f.body.translation().x > startX + 1, `${shape} must move through tank contact`);
      assert.ok(tank.body.translation().x > 0, `${shape} must not trap the scout`);
      assert.ok(tank.body.translation().y < 0.8, "tank stays grounded");
      assert.equal(tank.hp, hp);
      s.fragment(0, 0, 0x999999, 0.4);
      assert.equal(s.fragments.at(-1)!.body.collider(0).collisionGroups(), GROUP.fragment);
      f.life = DEBRIS_CLEANUP_SECONDS + STEP / 2;
      s.step();
      assert.equal(
        f.body.collider(0).collisionGroups(),
        GROUP.fragment,
        "sinking pieces cannot block tanks",
      );
      const life = f.life;
      const position = f.body.translation();
      blastDebris(s, position, 5, 100);
      assert.equal(f.life, life, "cleanup cannot be restarted by another blast");
      tick(s, 1.1);
      assert.equal(f.body.isValid(), false);
    } finally {
      s.dispose();
    }
  }
});

test("barrels rupture radially and a centered blast adds no sideways bias", () => {
  const s = arena();
  try {
    const c = cover(s, "drum");
    s.damageCover(c, 999, 999, 0);
    const scraps = s.fragments.filter((f) => f.shape === "drum-shell");
    assert.ok(scraps.some((f) => f.body.linvel().x < 0));
    assert.ok(scraps.some((f) => f.body.linvel().x > 0));
    for (const f of scraps) {
      const p = f.body.translation();
      const v = f.body.linvel();
      assert.ok((p.x - c.x) * v.x + (p.z - c.z) * v.z > 0);
      assert.ok(f.dimensions!.x < c.w / 2 && f.dimensions!.y < c.h / 2);
    }
    const lid = s.fragments.find((f) => f.shape === "drum-lid")!;
    lid.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
    blastDebris(s, c, 6, 75);
    assert.equal(lid.body.linvel().x, 0);
    assert.equal(lid.body.linvel().z, 0);
    assert.ok(lid.body.linvel().y > 0);
  } finally {
    s.dispose();
  }
});
