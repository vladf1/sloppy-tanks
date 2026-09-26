import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { spawnPositions } from "../src/game/arena";
import { coverDamageStage } from "../src/game/cover-model";
import { WEAPONS, STEP } from "../src/game/data";
import { Simulation } from "../src/game/simulation";
import { stepProjectiles } from "../src/game/weapons";
import type { Weapon } from "../src/game/types";
import { clearArena } from "./fixtures";

before(async () => {
  await RAPIER.init();
});

/** A shell from x = -3 heading east into a 10 m concrete wall at the origin. */
function wallShot(hp: number, weapon: Weapon, bounces = WEAPONS[weapon].bounces) {
  const s = clearArena(new Simulation(123));
  s.start();
  const cover = s.addCover({ kind: "concrete", x: 0, z: 0, w: 1, d: 10, h: 2, hp, color: 0 });
  s.world.step();
  s.shots.push({
    id: 999,
    x: -3,
    z: 0,
    vx: 180,
    vz: 0,
    owner: 999,
    team: 0,
    damage: WEAPONS[weapon].damage,
    bounces,
    life: 2,
    piercing: 0,
    weapon,
  });
  stepProjectiles(s, STEP);
  return { s, cover };
}

test("cover queries ignore tanks and debris and release destroyed collider identities", () => {
  const s = new Simulation(123);
  const c = s.addCover({ kind: "concrete", x: 0, z: 50, w: 1, d: 4, h: 2, hp: 10, color: 0 });
  const handle = c.collider.handle;
  s.human.body.setTranslation({ x: -2, y: 0.65, z: 50 }, true);
  s.fragment(2, 50, 0);
  s.world.step();
  const a = { x: -5, z: 50 },
    b = { x: 5, z: 50 };
  assert.equal(s.visible(a, b), false);
  assert.equal(s.coverByCollider.get(handle), c);
  s.damageCover(c, 10, s.human.id, s.humanTeam);
  assert.equal(s.coverByCollider.has(handle), false);
  assert.equal(s.visible(a, b), true);
  s.reset();
  assert.equal(s.coverByCollider.size, s.covers.length);
  for (const cover of s.covers) assert.equal(s.coverByCollider.get(cover.collider.handle), cover);
  s.dispose();
});

for (const weapon of ["standard", "spread", "ricochet"] as const)
  test(`${weapon} only reflects off surviving cover when using ricochet ammo`, () => {
    const { s, cover } = wallShot(200, weapon);
    assert.equal(cover.hp, 200 - WEAPONS[weapon].damage);
    if (weapon === "ricochet") {
      assert.equal(s.shots[0].bounces, 2);
      assert.ok(s.shots[0].vx < 0);
    } else {
      assert.equal(s.shots.length, 0);
      assert.equal(s.events.filter((e) => e.type === "ricochet").length, 0);
    }
    s.dispose();
  });

test("destroyed cover does not reflect shells and breaks exactly once", () => {
  // A shell that could still bounce is absorbed by the cover it destroys.
  const { s, cover } = wallShot(WEAPONS.standard.damage, "standard", 1);
  assert.equal(cover.alive, false);
  assert.equal(s.shots.length, 0);
  s.damageCover(cover, 100, 0, 0);
  assert.equal(s.destroyed, 1);
  s.dispose();
});

test("tower collapse opens center route and retains side rubble", () => {
  const s = new Simulation(123);
  const tower = s.covers.find((c) => c.kind === "tower")!;
  const before = s.nav.blocked[s.nav.index(tower)];
  const version = s.nav.version;
  s.damageCover(tower, 999, s.human.id, s.humanTeam);
  assert.equal(before, 1);
  assert.equal(s.nav.blocked[s.nav.index(tower)], 0);
  assert.ok(s.nav.version > version);
  assert.equal(s.covers.filter((c) => c.kind === "rubble").length, 2);
  const path = s.nav.find({ x: tower.x, z: tower.z - 6 }, { x: tower.x, z: tower.z + 6 });
  assert.ok(path.some((p) => Math.abs(p.x - tower.x) < 1 && Math.abs(p.z - tower.z) < 2));
  s.dispose();
});

test("destroyed village cover opens routes except rooted stumps, while all spawns reach midfield", () => {
  const s = new Simulation(123);
  for (const kind of ["house", "tree", "timber"] as const) {
    const c = s.covers.find(
      (c) => c.kind === kind && c.destructible && (kind !== "timber" || (c.x === -2 && c.z === 13)),
    )!;
    assert.ok(c && c.destructible);
    assert.equal(s.nav.blocked[s.nav.index(c)], 1);
    s.damageCover(c, 1000, s.human.id, s.humanTeam);
    assert.equal(c.alive, false);
    assert.equal(s.nav.blocked[s.nav.index(c)], kind === "tree" ? 1 : 0);
  }
  for (const team of [0, 1] as const)
    for (const p of spawnPositions(team)) assert.ok(s.nav.find(p, { x: 0, z: 0 }).length > 0);
  s.dispose();
});

test("harbor cargo stays solid while damaged, then opens collision and navigation; containers survive", () => {
  const sim = new Simulation(417);
  sim.mapMode = "harbor";
  sim.reset();
  try {
    sim.start();
    const cargo = sim.covers.find((c) => c.kind === "cargo" && c.x === 4)!;
    const container = sim.covers.find((c) => c.kind === "container")!;
    const handle = cargo.collider.handle;
    assert.equal(coverDamageStage(cargo), 0);
    assert.equal(sim.nav.blocked[sim.nav.index(cargo)], 1);
    sim.damageCover(cargo, 40, sim.human.id, sim.humanTeam);
    assert.equal(cargo.alive, true);
    assert.equal(coverDamageStage(cargo), 1);
    sim.damageCover(cargo, 40, sim.human.id, sim.humanTeam);
    assert.equal(coverDamageStage(cargo), 2);
    assert.equal(cargo.alive, true);
    assert.equal(sim.coverByCollider.has(handle), true, "damaged crates still stop shells");
    assert.equal(sim.nav.blocked[sim.nav.index(cargo)], 1, "damage does not open the route early");
    sim.damageCover(cargo, 20, sim.human.id, sim.humanTeam);
    assert.equal(cargo.alive, false);
    assert.equal(sim.coverByCollider.has(handle), false);
    assert.equal(sim.nav.blocked[sim.nav.index(cargo)], 0);
    assert.ok(sim.fragments.some((f) => f.shape === "panel" && f.material === "wood"));
    sim.damageCover(container, 10000, sim.human.id, sim.humanTeam);
    assert.equal(container.alive, true);
    assert.equal(sim.nav.blocked[sim.nav.index(container)], 1);
  } finally {
    sim.dispose();
  }
});
