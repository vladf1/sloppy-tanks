import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { stepProjectiles } from "../src/game/weapons";
import { Box3, Euler, Quaternion } from "three";
import { timberDamageStage, timberParts } from "../src/game/timber-layout";
import { timberPartModel } from "../src/game/timber-model";
import { disposeOwned } from "../src/game/render-resources";
import { GROUP, STEP } from "../src/game/data";
import { MAPS } from "../src/game/maps";
import { STRESS_TEST_MAP } from "../src/stress-test-level";
import { clearArena } from "./fixtures";

before(async () => {
  await RAPIER.init();
});

test("timber barriers meet without overlapping across every map and damaged corner pose", () => {
  for (const map of [...MAPS, STRESS_TEST_MAP]) {
    const walls = map.layout().filter((c) => c.kind === "timber");
    assert.ok(
      walls.every((wall) => wall.hp === 80),
      `${map.id}: timber takes two standard hits`,
    );
    const bounds = walls.map((wall) =>
      [0, 1, 2, 3].map((stage) =>
        timberParts(wall, stage).map((part) => {
          const model = timberPartModel(part);
          model.position.set(wall.x + part.x, part.y, wall.z + part.z);
          model.rotation.set(0, part.yaw, part.lean);
          const box = new Box3().setFromObject(model);
          disposeOwned(model);
          return box;
        }),
      ),
    );
    for (let i = 0; i < walls.length; i++) {
      for (let j = i + 1; j < walls.length; j++) {
        const a = walls[i],
          b = walls[j];
        assert.ok(
          Math.abs(a.x - b.x) >= (a.w + b.w) / 2 - 1e-6 ||
            Math.abs(a.z - b.z) >= (a.d + b.d) / 2 - 1e-6,
          `${map.id}: overlapping timber colliders at ${a.x},${a.z} and ${b.x},${b.z}`,
        );
        const corner = a.timberJoin?.post || b.timberJoin?.post || a.w > a.d !== b.w > b.d;
        // Include straps and all independent damage poses at perpendicular corners.
        for (const boxesA of corner ? bounds[i] : bounds[i].slice(0, 1)) {
          for (const boxesB of corner ? bounds[j] : bounds[j].slice(0, 1)) {
            assert.ok(
              boxesA.every((boxA) => boxesB.every((boxB) => !boxA.intersectsBox(boxB))),
              `${map.id}: overlapping timber models at ${a.x},${a.z} and ${b.x},${b.z}`,
            );
          }
        }
      }
    }
  }
});

test("two shells breach one timber bay, clearing physics and bot navigation; reset restores it", () => {
  const s = new Simulation(123);
  try {
    for (const t of s.tanks) {
      s.world.removeRigidBody(t.body);
    }
    s.tanks = [];
    const wall = s.covers.find((c) => c.kind === "timber" && c.x === -2 && c.z === 13)!;
    const neighbor = s.covers.find((c) => c.kind === "timber" && c.x === 2 && c.z === 13)!;
    assert.ok(wall && neighbor);
    const handle = wall.collider.handle,
      version = s.nav.version;
    assert.equal(s.nav.blocked[s.nav.index(wall)], 1);
    for (let hit = 1; hit <= 2; hit++) {
      s.shots = [
        {
          id: s.nextId++,
          x: wall.x,
          z: wall.z - 5,
          vx: 0,
          vz: 600,
          owner: 999,
          team: 0,
          damage: 40,
          bounces: 0,
          life: 2,
          piercing: 0,
          weapon: "standard",
        },
      ];
      stepProjectiles(s, STEP);
      assert.equal(wall.hp, 80 - hit * 40);
      assert.equal(wall.alive, hit < 2);
      assert.equal(s.shots.length, 0);
    }
    assert.equal(neighbor.hp, 80);
    assert.equal(neighbor.alive, true);
    assert.equal(s.coverByCollider.has(handle), false);
    assert.ok(s.nav.version > version);
    assert.equal(s.nav.blocked[s.nav.index(wall)], 0);
    s.world.step();
    assert.equal(s.visible({ x: wall.x, z: wall.z - 3 }, { x: wall.x, z: wall.z + 3 }), true);
    assert.ok(s.fragments.length > 0);
    s.reset();
    assert.ok(s.covers.filter((c) => c.kind === "timber").every((c) => c.alive && c.hp === 80));
  } finally {
    s.dispose();
  }
});

test("all eight garden corners share one upright and adjoining runs break independently", () => {
  const s = new Simulation(123);
  try {
    const posts = s.covers.filter((c) => c.timberJoin?.post);
    assert.equal(posts.length, 8);
    for (const post of posts) {
      const parts = timberParts(post, 0);
      assert.equal(parts.length, 1);
      assert.equal(parts[0].kind, "post");
      assert.equal(parts[0].w, post.w);
      assert.equal(parts[0].d, post.d);
      const neighbors = s.covers.filter((c) => {
        if (c.kind !== "timber" || c.timberJoin?.post) return false;
        const gapX = Math.abs(c.x - post.x) - (c.w + post.w) / 2;
        const gapZ = Math.abs(c.z - post.z) - (c.d + post.d) / 2;
        return (
          (Math.abs(gapX - 0.04) < 1e-6 && Math.abs(c.z - post.z) < 1e-6) ||
          (Math.abs(gapZ - 0.04) < 1e-6 && Math.abs(c.x - post.x) < 1e-6)
        );
      });
      assert.equal(neighbors.length, 2, "two perpendicular runs end at each connector");
      for (const neighbor of neighbors) {
        assert.equal(timberParts(neighbor, 0).length, 5, "no duplicate corner post on the run");
      }
      s.damageCover(neighbors[0], 80, 999, 0);
      assert.equal(post.alive, true);
      assert.equal(neighbors[1].hp, 80);
      const count = s.fragments.length;
      s.damageCover(post, 40, 999, 0, undefined, { x: post.x, y: 1, z: post.z - 0.45 });
      assert.equal(post.alive, true);
      assert.equal(timberParts(post, 2)[0].marks.length, 1);
      s.damageCover(post, 40, 999, 0);
      assert.equal(post.alive, false);
      assert.equal(neighbors[1].alive, true);
      assert.equal(s.fragments.length, count + 1, "connector becomes exactly one physical post");
      const fragment = s.fragments.at(-1)!;
      assert.equal(fragment.timberPart?.kind, "post");
      assert.deepEqual(fragment.dimensions, { x: post.w, y: post.h, z: post.d });
      assert.equal(fragment.body.collider(0).collisionGroups(), GROUP.timberDebris);
    }
  } finally {
    s.dispose();
  }
});

test("blast can destroy adjacent timber bays without duplicate destruction", () => {
  const s = new Simulation(123);
  try {
    const bays = s.covers.filter((c) => c.kind === "timber" && c.z === 13 && Math.abs(c.x) === 2);
    s.explode({ x: 0, z: 13 }, 5, 120, s.human.id, s.humanTeam);
    for (const bay of bays) {
      assert.equal(bay.alive, false);
      s.damageCover(bay, 999, s.human.id, s.humanTeam);
      assert.equal(s.events.filter((e) => e.type === "destroy" && e.id === bay.id).length, 1);
    }
  } finally {
    s.dispose();
  }
});

test("timber uses four beams and two posts, retaining their damage and pose on breakup", () => {
  for (const along of [true, false]) {
    const s = new Simulation(123);
    try {
      const wall = s.addCover({
        kind: "timber",
        x: 0,
        z: 0,
        w: along ? 4.2 : 0.9,
        d: along ? 0.9 : 4.2,
        h: 2.8,
        hp: 120,
        color: 0xb47a49,
      });
      s.damageCover(wall, 105, 999, 0, undefined, { x: 0.7, y: 1, z: -0.45 });
      assert.equal(wall.body.numColliders(), 1);
      assert.ok(wall.body.isFixed());
      const stage = timberDamageStage(wall.hp, wall.maxHp);
      assert.equal(stage, 3);
      const parts = timberParts(wall, stage);
      assert.equal(parts.filter((p) => p.kind === "beam").length, 4);
      assert.equal(parts.filter((p) => p.kind === "post").length, 2);
      s.damageCover(wall, 15, 999, 0);
      assert.equal(s.fragments.length, 6);
      for (let i = 0; i < parts.length; i++) {
        const fragment = s.fragments[i],
          part = parts[i];
        assert.deepEqual(fragment.timberPart, part);
        const p = fragment.body.translation();
        assert.ok(Math.abs(p.x - (wall.x + part.x)) < 1e-5);
        assert.ok(Math.abs(p.y - part.y) < 1e-5);
        assert.ok(Math.abs(p.z - (wall.z + part.z)) < 1e-5);
        assert.deepEqual(fragment.dimensions, { x: part.w, y: part.h, z: part.d });
        const expected = new Quaternion().setFromEuler(new Euler(0, part.yaw, part.lean));
        const q = fragment.body.rotation();
        assert.ok(Math.abs(expected.dot(new Quaternion(q.x, q.y, q.z, q.w))) > 0.99999);
        assert.equal(fragment.body.numColliders(), 1);
        assert.equal(fragment.body.collider(0).collisionGroups(), GROUP.timberDebris);
      }
    } finally {
      s.dispose();
    }
  }
});

test("detached timber beams land across one another and remain stacked", () => {
  const s = new Simulation(123);
  try {
    clearArena(s);
    const wall = s.addCover({
      kind: "timber",
      x: 0,
      z: 0,
      w: 4.2,
      d: 0.9,
      h: 2.8,
      hp: 120,
      color: 0xb47a49,
    });
    s.damageCover(wall, 120, 999, 0);
    const [lower, upper] = s.fragments;
    for (const piece of s.fragments.slice(2)) s.world.removeRigidBody(piece.body);
    s.fragments = [lower, upper];
    for (const [i, piece] of s.fragments.entries()) {
      piece.body.setTranslation({ x: 0, y: 1 + i * 2, z: 0 }, true);
      piece.body.setRotation(
        { x: 0, y: i ? Math.SQRT1_2 : 0, z: 0, w: i ? Math.SQRT1_2 : 1 },
        true,
      );
      piece.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
      piece.body.setAngvel({ x: 0, y: 0, z: 0 }, true);
    }
    for (let i = 0; i < 480; i++) s.world.step();
    const a = lower.body.translation(),
      b = upper.body.translation();
    const thickness = lower.dimensions!.y;
    assert.ok(Math.abs(a.y - thickness / 2) < 0.03, "lower beam rests on the ground");
    assert.ok(b.y - a.y > thickness - 0.03, "upper beam rests on the lower beam, not through it");
    assert.ok(Math.abs(b.x) < 0.1 && Math.abs(b.z) < 0.1, "crossed beams keep their support");
    assert.ok(lower.body.isSleeping() && upper.body.isSleeping(), "the pile settles");
    const allows = (a: number, b: number) =>
      ((a >>> 16) & b & 0xffff) !== 0 && ((b >>> 16) & a & 0xffff) !== 0;
    assert.ok(allows(GROUP.timberDebris, GROUP.timberDebris));
    assert.ok(allows(GROUP.timberDebris, GROUP.tank));
    for (const excluded of [GROUP.fragment, GROUP.coverQuery, GROUP.steeringQuery]) {
      assert.equal(allows(GROUP.timberDebris, excluded), false);
    }
  } finally {
    s.dispose();
  }
});
