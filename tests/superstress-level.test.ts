import assert from "node:assert/strict";
import { before, test } from "node:test";
import RAPIER from "@dimforge/rapier3d-compat";
import { ARENA, STEP } from "../src/game/data";
import { DEBRIS_CLEANUP_SECONDS } from "../src/game/debris-cleanup";
import { Simulation } from "../src/game/simulation";
import type { Cover, CoverKind } from "../src/game/types";
import {
  REBUILD_SECONDS,
  SUPERSTRESS_MAP,
  SUPERSTRESS_SCALE,
  SUPERSTRESS_SETUP,
  superstressRules,
} from "../src/superstress-level";

const YARD = ARENA * SUPERSTRESS_SCALE;

before(async () => {
  await RAPIER.init();
});

test("superstress spawns and pickups fit the compact yard with hull clearance and routes", () => {
  const sim = new Simulation(731, { ...SUPERSTRESS_SETUP, round: 3 });
  try {
    assert.equal(sim.tanks.length, 30);
    const points = [...sim.pickups, ...sim.tanks.map((tank) => tank.body.translation())];
    for (const point of points) {
      assert.ok(Math.max(Math.abs(point.x), Math.abs(point.z)) < YARD - 2, "inside the yard");
      assert.equal(sim.nav.blocked[sim.nav.index(point)], 0, `blocked ${JSON.stringify(point)}`);
      assert.ok(
        (point.x === 0 && point.z === 0) || sim.nav.find({ x: 0, z: 0 }, point).length > 0,
        `unreachable ${JSON.stringify(point)}`,
      );
      for (const cover of sim.covers) {
        assert.ok(
          Math.abs(point.x - cover.x) >= cover.w / 2 + 1.5 ||
            Math.abs(point.z - cover.z) >= cover.d / 2 + 1.5,
          `no hull clearance at ${JSON.stringify(point)} beside ${cover.kind}`,
        );
      }
    }
  } finally {
    sim.dispose();
  }
});

test("the yard is dense, half-turn symmetric and has only cover that rebuilds in place", () => {
  const layout = SUPERSTRESS_MAP.layout();
  const destructible = layout.filter((cover) => Number.isFinite(cover.hp));
  assert.ok(destructible.length >= 100);
  assert.equal(layout.filter((cover) => cover.kind === "boundary").length, 4);
  // A collapsing tower adds separate rubble covers, which a rebuild could never reclaim.
  assert.ok(destructible.every((cover) => cover.kind !== "tower"));
  const key = (kind: CoverKind, x: number, z: number) => `${kind}:${x.toFixed(3)}:${z.toFixed(3)}`;
  const keys = new Set(layout.map((cover) => key(cover.kind, cover.x, cover.z)));
  assert.equal(keys.size, layout.length, "every obstacle needs a unique location and kind");
  for (const cover of layout) {
    assert.ok(keys.has(key(cover.kind, -cover.x, -cover.z)), `${cover.kind} has a rotated twin`);
    if (cover.kind !== "boundary") {
      assert.ok(Math.abs(cover.x) + cover.w / 2 <= YARD && Math.abs(cover.z) + cover.d / 2 <= YARD);
    }
  }
});

function destroy(sim: Simulation, kind: CoverKind): Cover {
  const cover = sim.covers.find((candidate) => candidate.kind === kind && candidate.alive)!;
  sim.damageCover(cover, 9999, sim.human.id, sim.human.team);
  assert.equal(cover.alive, false);
  return cover;
}

test("destroyed cover rises with its identity once the rebuild delay passes and it is clear", () => {
  const sim = new Simulation(731, { ...SUPERSTRESS_SETUP, round: 3 });
  try {
    const coverCount = sim.covers.length;
    const bodies = sim.world.bodies.len();
    // Push the drum off its spot first, so the rebuild has to return it to its origin.
    const drum = sim.covers.find((cover) => cover.kind === "drum")!;
    const origin = { x: drum.x, z: drum.z };
    drum.body.setTranslation({ x: origin.x + 2, y: drum.h / 2, z: origin.z }, true);
    const fallen = [
      destroy(sim, "timber"),
      destroy(sim, "tree"),
      destroy(sim, "cargo"),
      (sim.damageCover(drum, 9999, sim.human.id, sim.human.team), drum),
    ];
    sim.events = [];
    const nearTank = sim.tanks.find((tank) => !tank.human)!;
    nearTank.body.setTranslation({ x: origin.x, y: 0.65, z: origin.z + 1 }, true);
    const rng = sim.rng.state;
    superstressRules(sim);
    sim.elapsed += REBUILD_SECONDS - 0.5;
    superstressRules(sim);
    assert.ok(
      fallen.every((cover) => !cover.alive),
      "still waiting for the rebuild delay",
    );

    sim.elapsed += 0.5;
    superstressRules(sim);
    assert.equal(drum.alive, false, "a tank on the footprint delays the rebuild");
    for (const cover of fallen.filter((cover) => cover !== drum)) {
      assert.equal(cover.alive, true, `${cover.kind} rebuilt`);
    }
    nearTank.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    superstressRules(sim);

    for (const cover of fallen) {
      assert.equal(cover.alive, true);
      assert.equal(cover.hp, cover.maxHp);
      assert.equal(cover.timberHits, undefined);
      assert.ok(sim.covers.includes(cover), `${cover.kind} keeps its record`);
      assert.equal(sim.coverByCollider.get(cover.collider.handle), cover);
      assert.equal(sim.nav.blocked[sim.nav.index(cover)], 1, `${cover.kind} blocks routes again`);
      assert.ok(
        sim.events.some((event) => event.type === "impact" && event.id === cover.id),
        `${cover.kind} announces its rebuild`,
      );
    }
    assert.deepEqual({ x: drum.x, z: drum.z }, origin);
    const p = drum.body.translation();
    assert.ok(Math.hypot(p.x - origin.x, p.z - origin.z) < 1e-6, "the drum returns to its origin");
    assert.equal(sim.movableCovers.filter((cover) => cover === drum).length, 1);
    // Destruction debris is the only growth: no stumps or dead bodies are left behind.
    assert.equal(sim.covers.length, coverCount);
    assert.equal(sim.world.bodies.len(), bodies + sim.fragments.length);
    assert.equal(sim.rng.state, rng, "rules and rebuilds draw no gameplay randomness");
  } finally {
    sim.dispose();
  }
});

test("debris lingers while the fragment budget has room, then resumes the normal fade", () => {
  const sim = new Simulation(731, { ...SUPERSTRESS_SETUP, round: 3 });
  try {
    sim.fragment(0, 0, 0xffffff);
    const piece = sim.fragments[0];
    piece.life = DEBRIS_CLEANUP_SECONDS + 0.1;
    superstressRules(sim);
    assert.ok(piece.life > DEBRIS_CLEANUP_SECONDS + 1, "settling debris is held");

    while (sim.fragments.length < sim.maxFragments * 0.8) {
      sim.fragment(0, 0, 0xffffff);
    }
    piece.life = DEBRIS_CLEANUP_SECONDS + 0.1;
    superstressRules(sim);
    assert.equal(piece.life, DEBRIS_CLEANUP_SECONDS + 0.1, "a full budget lets debris fade");
  } finally {
    sim.dispose();
  }
});

test("a seeded superstress brawl keeps rebuilding its cover and stays inside its bounds", () => {
  const sim = new Simulation(4242, { ...SUPERSTRESS_SETUP, round: 3 });
  try {
    const coverCount = sim.covers.length;
    const restored = new Set<number>();
    sim.start();
    for (let i = 0; i < 10 / STEP; i++) {
      sim.step(undefined, true);
      for (const event of sim.events.splice(0)) {
        if (event.type === "impact" && event.id !== undefined && event.coverKind) {
          restored.add(event.id);
        }
      }
      assert.ok(sim.fragments.length <= sim.maxFragments);
    }
    assert.ok(sim.destroyed > 15, `only ${sim.destroyed} covers fell`);
    assert.ok(restored.size > 5, `only ${restored.size} covers rebuilt`);
    assert.equal(sim.covers.length, coverCount);
    for (const tank of sim.tanks.filter((tank) => tank.alive)) {
      const p = tank.body.translation();
      assert.ok(Math.max(Math.abs(p.x), Math.abs(p.z)) < YARD, "tanks stay inside the fence");
    }
  } finally {
    sim.dispose();
  }
});
