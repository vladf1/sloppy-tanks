import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { pickupLayout, spawnPositions } from "../src/game/arena";
import { harborLayout } from "../src/game/harbor-layout";
import { coverDamageStage } from "../src/game/cover-model";
import { Simulation } from "../src/game/simulation";

before(async () => {
  await RAPIER.init();
});

function harbor(mode: "team" | "solo" = "team") {
  const sim = new Simulation(417);
  sim.mapMode = "harbor";
  sim.gameMode = mode;
  sim.reset();
  return sim;
}

test("harbor cover is balanced and every pickup, spawn and quay route is reachable", () => {
  const sim = harbor();
  try {
    const layout = harborLayout();
    for (const c of layout) {
      assert.ok(
        layout.some(
          (o) =>
            o.kind === c.kind &&
            o.x === -c.x &&
            o.z === -c.z &&
            o.w === c.w &&
            o.d === c.d &&
            o.hp === c.hp,
        ),
        `unpaired cover ${JSON.stringify(c)}`,
      );
    }
    const points = [
      ...pickupLayout,
      ...spawnPositions(0),
      ...spawnPositions(1),
      ...[-52, 52].flatMap((z) => [-45, 0, 45].map((x) => ({ x, z }))),
    ];
    for (const point of points) {
      assert.equal(sim.nav.blocked[sim.nav.index(point)], 0, `blocked ${JSON.stringify(point)}`);
      assert.ok(
        sim.nav.find({ x: -53, z: 0 }, point).length > 0 || (point.x === -53 && point.z === 0),
        `unreachable ${JSON.stringify(point)}`,
      );
      for (const cover of layout) {
        assert.ok(
          Math.abs(point.x - cover.x) >= cover.w / 2 + 1.5 ||
            Math.abs(point.z - cover.z) >= cover.d / 2 + 1.5,
          `hull clearance ${JSON.stringify(point)}`,
        );
      }
    }
    for (const tank of sim.tanks)
      assert.equal(sim.nav.blocked[sim.nav.index(tank.body.translation())], 0);
  } finally {
    sim.world.free();
  }
});

test("cargo destruction removes collision and opens its navigation footprint; containers survive", () => {
  const sim = harbor();
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
    assert.ok(sim.fragments.some((f) => f.shape === "wood"));
    sim.damageCover(container, 10000, sim.human.id, sim.humanTeam);
    assert.equal(container.alive, true);
    assert.equal(sim.nav.blocked[sim.nav.index(container)], 1);
  } finally {
    sim.world.free();
  }
});

test("Harbor Havoc supports both modes and round/map resets restore their own cover", () => {
  for (const mode of ["team", "solo"] as const) {
    const sim = harbor(mode);
    try {
      assert.equal(sim.mapName, "HARBOR HAVOC");
      assert.equal(sim.tanks.length, mode === "solo" ? 7 : 12);
      const initial = sim.world.bodies.len();
      sim.start();
      for (let i = 0; i < 900; i++) sim.step(undefined, true);
      assert.ok(sim.shotsFired > 0, "bots engage on the new layout");
      for (const tank of sim.tanks) {
        if (!tank.alive) continue;
        const p = tank.body.translation();
        assert.ok(Number.isFinite(p.x) && Number.isFinite(p.z));
        assert.ok(Math.abs(p.x) < 60 && Math.abs(p.z) < 60, "quay walls contain tanks");
      }
      sim.reset();
      assert.equal(sim.world.bodies.len(), initial);
      assert.ok(sim.covers.filter((c) => c.kind === "cargo").every((c) => c.alive));
      sim.mapMode = "village";
      sim.reset();
      assert.ok(sim.covers.some((c) => c.kind === "tree"));
      assert.ok(!sim.covers.some((c) => c.kind === "container"));
      sim.mapMode = "random";
      sim.reset();
      assert.ok(sim.covers.some((c) => c.kind === "container"));
      sim.mapMode = "harbor";
      sim.reset();
      assert.equal(sim.world.bodies.len(), initial);
    } finally {
      sim.world.free();
    }
  }
});
