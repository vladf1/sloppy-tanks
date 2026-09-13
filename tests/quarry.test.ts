import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { pickupLayout, spawnPositions } from "../src/game/arena";
import { quarryLayout } from "../src/game/quarry-layout";
import { Simulation } from "../src/game/simulation";

before(async () => {
  await RAPIER.init();
});
function quarry() {
  const sim = new Simulation(417);
  sim.mapMode = "quarry";
  sim.reset();
  return sim;
}

test("Dusty Dig is balanced with clear pickups, deployment and connected outer routes", () => {
  const sim = quarry();
  try {
    const layout = quarryLayout();
    for (const c of layout) {
      assert.ok(
        layout.some(
          (other) =>
            other.kind === c.kind &&
            other.x === -c.x &&
            other.z === -c.z &&
            other.w === c.w &&
            other.d === c.d &&
            other.hp === c.hp,
        ),
        `unpaired ${JSON.stringify(c)}`,
      );
    }
    for (const point of [
      ...pickupLayout,
      ...spawnPositions(0),
      ...spawnPositions(1),
      ...[-53, 53].flatMap((z) => [-45, 0, 45].map((x) => ({ x, z }))),
    ]) {
      assert.equal(sim.nav.blocked[sim.nav.index(point)], 0, `blocked ${JSON.stringify(point)}`);
      assert.ok(
        sim.nav.find({ x: -53, z: 0 }, point).length || (point.x === -53 && point.z === 0),
        `unreachable ${JSON.stringify(point)}`,
      );
      for (const c of layout) {
        assert.ok(
          Math.abs(point.x - c.x) >= c.w / 2 + 1.5 || Math.abs(point.z - c.z) >= c.d / 2 + 1.5,
          `hull clearance ${JSON.stringify(point)} / ${c.kind}`,
        );
      }
    }
    assert.equal(
      sim.nav.clearLine({ x: -48, z: 0 }, { x: 48, z: 0 }),
      true,
      "central crossing remains open",
    );
  } finally {
    sim.world.free();
  }
});

test("quarry crate cuts open to tanks only after destruction; barriers and rock survive", () => {
  const sim = quarry();
  try {
    const a = { x: 32.5, z: 26 };
    const b = { x: 32.5, z: 48 };
    assert.equal(sim.nav.clearLine(a, b), false);
    for (const crate of sim.covers.filter((c) => c.kind === "cargo" && c.x > 30 && c.z > 30)) {
      sim.damageCover(crate, 30, sim.human.id, sim.humanTeam);
      assert.equal(sim.nav.blocked[sim.nav.index(crate)], 1);
      sim.damageCover(crate, 30, sim.human.id, sim.humanTeam);
      assert.equal(sim.coverByCollider.has(crate.collider.handle), false);
    }
    assert.equal(sim.nav.clearLine(a, b), true, "all four supply crates open the rock cut");
    for (const c of sim.covers.filter((c) => ["rock", "teeth", "hedgehog"].includes(c.kind))) {
      sim.damageCover(c, 10000, sim.human.id, sim.humanTeam);
      assert.equal(c.alive, true);
      assert.equal(sim.nav.blocked[sim.nav.index(c)], 1);
    }
  } finally {
    sim.world.free();
  }
});

test("barrier collision follows tapered concrete and open steel rather than invisible boxes", () => {
  const sim = quarry();
  try {
    sim.world.step();
    const tooth = sim.covers.filter((c) => c.kind === "teeth").sort((a, b) => a.x - b.x)[0];
    const hedgehog = sim.covers.filter((c) => c.kind === "hedgehog").sort((a, b) => a.x - b.x)[0];
    const ray = (cover: typeof tooth, x: number, y: number) =>
      cover.collider.castRay(
        new RAPIER.Ray({ x: cover.x + x, y, z: cover.z - 4 }, { x: 0, y: 0, z: 1 }),
        8,
        true,
      );
    assert.ok(ray(tooth, 0, 1) >= 0);
    assert.equal(ray(tooth, 0.8, 1.7), -1, "shot clears the sloping shoulder");
    assert.ok(ray(hedgehog, 0, 1.3) >= 0, "central steel stops a shot");
    assert.equal(ray(hedgehog, 0.95, 1.3), -1, "visible opening between steel arms remains open");
    const tank = sim.human;
    tank.body.setTranslation({ x: tooth.x - 4, y: 0.65, z: tooth.z }, true);
    for (let i = 0; i < 180; i++) {
      tank.body.setLinvel({ x: 7, y: 0, z: 0 }, true);
      sim.world.step();
    }
    assert.ok(tank.body.translation().x < tooth.x, "a tank cannot drive through dragon teeth");
    tank.body.setTranslation({ x: hedgehog.x - 4, y: 0.65, z: hedgehog.z }, true);
    for (let i = 0; i < 180; i++) {
      tank.body.setLinvel({ x: 7, y: 0, z: 0 }, true);
      sim.world.step();
    }
    assert.ok(
      tank.body.translation().x < hedgehog.x,
      "a tank cannot drive through steel hedgehogs",
    );
  } finally {
    sim.world.free();
  }
});

test("quarry supports both modes, combat, resets and Surprise me selection", () => {
  const sim = quarry();
  try {
    for (const mode of ["team", "solo"] as const) {
      sim.gameMode = mode;
      sim.reset();
      sim.start();
      const bodies = sim.world.bodies.len();
      for (let i = 0; i < 1200; i++) sim.step(undefined, true);
      assert.ok(sim.shotsFired > 20);
      assert.ok(
        sim.tanks.some((tank) => tank.deaths > 0),
        "bots can resolve fights",
      );
      for (const t of sim.tanks.filter((t) => t.alive)) {
        const p = t.body.translation();
        assert.ok(Number.isFinite(p.x) && Number.isFinite(p.z));
        assert.ok(Math.abs(p.x) < 60 && Math.abs(p.z) < 60);
      }
      sim.reset();
      assert.equal(sim.world.bodies.len(), bodies);
      assert.equal(sim.mapName, "DUSTY DIG");
    }
    const themes = new Set<string>();
    sim.mapMode = "surprise";
    for (let i = 0; i < 30; i++) {
      sim.reset();
      themes.add(sim.mapTheme);
    }
    assert.deepEqual([...themes].sort(), ["harbor", "quarry", "village"]);
  } finally {
    sim.world.free();
  }
});

test("quarry defenses form mirrored belts and supply bays use individual crates", () => {
  const layout = quarryLayout();
  for (const side of [-1, 1]) {
    const teeth = layout.filter((c) => c.kind === "teeth" && Math.sign(c.x) === side);
    assert.equal(teeth.length, 8);
    assert.equal(new Set(teeth.map((c) => c.x)).size, 2, "two staggered ranks");
    const steel = layout.filter((c) => c.kind === "hedgehog" && Math.sign(c.x) === side);
    assert.equal(steel.length, 4);
    assert.equal(new Set(steel.map((c) => c.z)).size, 1, "one aligned steel belt");
    for (const barrier of [...teeth, ...steel]) {
      assert.ok(
        [...teeth, ...steel].some(
          (other) =>
            other !== barrier &&
            other.kind === barrier.kind &&
            Math.hypot(other.x - barrier.x, other.z - barrier.z) < 3.6,
        ),
        "every obstacle belongs to a connected barrier",
      );
    }
    const crates = layout.filter((c) => c.kind === "cargo" && c.x * side > 30 && c.z * side > 30);
    assert.equal(crates.length, 4);
    assert.ok(crates.every((c) => c.w <= 2.4 && c.d <= 2.8));
    for (const crate of crates) {
      assert.ok(
        layout
          .filter((c) => c.kind === "rock")
          .every(
            (rock) =>
              Math.abs(crate.x - rock.x) >= (crate.w + rock.w) / 2 ||
              Math.abs(crate.z - rock.z) >= (crate.d + rock.d) / 2,
          ),
        "supply crates never intersect rock footprints",
      );
    }
  }
});
