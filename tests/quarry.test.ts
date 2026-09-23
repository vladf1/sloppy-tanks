import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { pickupLayout, spawnPositions } from "../src/game/arena";
import {
  quarryButteFootprint,
  quarryButteSpot,
  quarryScreeSpots,
  quarryStockpileGeometry,
  quarryStockpileSpot,
  quarryTalusGeometry,
  quarryTalusStrips,
} from "../src/game/quarry-benches";
import { QUARRY_RAMP } from "../src/game/quarry-ramp";
import { quarrySpawnPadPieces, type SpawnPadPiece } from "../src/game/quarry-scenery";
import { quarryLayout } from "../src/game/quarry-layout";
import { GROUP } from "../src/game/data";
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
      sim.world.castRay(
        new RAPIER.Ray({ x: cover.x + x, y, z: cover.z - 4 }, { x: 0, y: 0, z: 1 }),
        8,
        true,
        undefined,
        GROUP.coverQuery,
        undefined,
        undefined,
        (collider) => collider.parent()?.handle === cover.body.handle,
      )?.timeOfImpact ?? -1;
    assert.ok(ray(tooth, 0, 1) >= 0);
    assert.equal(ray(tooth, 0.8, 1.7), -1, "shot clears the sloping shoulder");
    assert.equal(
      sim.world.castRay(
        new RAPIER.Ray({ x: tooth.x + 0.8, y: 1.7, z: tooth.z - 4 }, { x: 0, y: 0, z: 1 }),
        8,
        true,
        undefined,
        GROUP.coverQuery,
        undefined,
        undefined,
        (collider) => collider.parent()?.handle === tooth.body.handle,
      ),
      null,
      "the tank footprint never creates invisible cover for shells",
    );
    assert.ok(ray(hedgehog, 0, 1.3) >= 0, "central steel stops a shot");
    assert.equal(ray(hedgehog, 0.95, 1.3), -1, "visible opening between steel arms remains open");
    const tank = sim.human;
    tank.body.setTranslation({ x: tooth.x - 4, y: 0.65, z: tooth.z }, true);
    for (let i = 0; i < 180; i++) {
      tank.body.setLinvel({ x: 7, y: 0, z: 0 }, true);
      sim.world.step();
    }
    assert.ok(
      tank.body.translation().x < tooth.body.translation().x,
      "a tank pushes concrete but cannot pass through it",
    );
    tank.body.setTranslation({ x: hedgehog.x - 4, y: 0.65, z: hedgehog.z }, true);
    for (let i = 0; i < 180; i++) {
      tank.body.setLinvel({ x: 7, y: 0, z: 0 }, true);
      sim.world.step();
    }
    assert.ok(
      tank.body.translation().x < hedgehog.body.translation().x ||
        Math.hypot(
          hedgehog.body.translation().x - hedgehog.x,
          hedgehog.body.translation().z - hedgehog.z,
        ) > 0.8,
      "a tank remains blocked unless it physically pushes the steel aside",
    );
  } finally {
    sim.world.free();
  }
});

test("quarry supports both modes, combat and resets", () => {
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
  } finally {
    sim.world.free();
  }
});

test("quarry defenses form mirrored belts and supply bays use individual crates", () => {
  const layout = quarryLayout();
  for (const side of [-1, 1]) {
    const teeth = layout.filter((c) => c.kind === "teeth" && Math.sign(c.x) === side);
    assert.equal(teeth.length, 8);
    assert.equal(teeth.filter((c) => Math.abs(c.x) < 43).length, 4, "inner staggered rank");
    assert.equal(teeth.filter((c) => Math.abs(c.x) > 44).length, 4, "outer staggered rank");
    assert.ok(
      new Set(teeth.map((c) => c.x)).size > 4,
      "individual placement breaks straight lines",
    );
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

test("scree collapses and the sentinel butte stay outside the playable boundary", () => {
  const spots = quarryScreeSpots();
  assert.ok(spots.length >= 4, "collapses interrupt several terraces");
  assert.ok(
    new Set(spots.map((s) => s.seed)).size === spots.length,
    "each collapse has its own relief",
  );
  for (const spot of spots) {
    // The rear of each solid pile is buried inside the first quarry cut.
    const corners = [
      [-spot.length / 2, 0],
      [spot.length / 2, 0],
      [-spot.length / 2, spot.depth],
      [spot.length / 2, spot.depth],
    ].map(([lx, lz]) => [
      spot.x + lx * Math.cos(spot.rotY) + lz * Math.sin(spot.rotY),
      spot.z - lx * Math.sin(spot.rotY) + lz * Math.cos(spot.rotY),
    ]);
    for (const [x, z] of corners) {
      assert.ok(
        Math.max(Math.abs(x), Math.abs(z)) > 60,
        `scree corner inside the arena: ${x.toFixed(1)},${z.toFixed(1)}`,
      );
      assert.ok(
        Math.max(Math.abs(x), Math.abs(z)) < 85,
        `scree corner adrift from its terrace: ${x.toFixed(1)},${z.toFixed(1)}`,
      );
    }
  }
  const butte = quarryButteSpot();
  assert.ok(
    Math.max(Math.abs(butte.x), Math.abs(butte.z)) > 60,
    "the sentinel stands on the apron, not in the arena",
  );
  for (const [x, z] of quarryButteFootprint(butte)) {
    assert.ok(
      Math.max(Math.abs(x), Math.abs(z)) > 60.5,
      `butte slab inside the boundary wall: ${x.toFixed(1)},${z.toFixed(1)}`,
    );
  }
});

test("wall talus and the conveyor stockpile stay on the apron, clear of the haul ramp", () => {
  const strips = quarryTalusStrips();
  assert.equal(strips.length, 4, "every lowest cut has a talus toe");
  for (const strip of strips) {
    const talus = quarryTalusGeometry(strip.x0, strip.x1, strip.seed);
    try {
      talus.rotateY(strip.rotY);
      talus.translate(strip.x, 0, strip.z);
      const positions = talus.getAttribute("position");
      let outermost = 0;
      for (let i = 0; i < positions.count; i++) {
        const x = positions.getX(i);
        const z = positions.getZ(i);
        const extent = Math.max(Math.abs(x), Math.abs(z));
        assert.ok(extent > 70, `talus spills toward the arena: ${x.toFixed(1)},${z.toFixed(1)}`);
        assert.ok(
          !(x > QUARRY_RAMP.x0 && x < QUARRY_RAMP.x1 && z > QUARRY_RAMP.z0 && z < QUARRY_RAMP.z1),
          `talus buries the haul ramp: ${x.toFixed(1)},${z.toFixed(1)}`,
        );
        outermost = Math.max(outermost, extent);
      }
      assert.ok(outermost > 77.5, "the talus tucks into its wall instead of ending short");
    } finally {
      talus.dispose();
    }
  }
  const pile = quarryStockpileSpot();
  const stockpile = quarryStockpileGeometry(pile);
  try {
    stockpile.translate(pile.x, 0, pile.z);
    const positions = stockpile.getAttribute("position");
    for (let i = 0; i < positions.count; i++) {
      const extent = Math.max(Math.abs(positions.getX(i)), Math.abs(positions.getZ(i)));
      assert.ok(extent > 62, "the stockpile toe stays behind the boundary wall");
    }
  } finally {
    stockpile.dispose();
  }
});

test("spawn pads stay flat, compact and mirrored between teams", () => {
  const pads = [quarrySpawnPadPieces(0), quarrySpawnPadPieces(1)];
  for (const pieces of pads) {
    assert.ok(pieces.length > 10, "pads have graded detail, not a bare disc");
    for (const piece of pieces) {
      assert.ok(
        piece.y + piece.h / 2 <= 1.1,
        `${piece.shape} rises above paint height and could read as cover`,
      );
      assert.ok(
        Math.hypot(piece.dx, piece.dz) + Math.max(piece.w, piece.d) / 2 <= 3.1,
        `${piece.shape} sprawls past its pad into the combat lanes`,
      );
    }
  }
  for (const pieces of pads) {
    const chevrons = pieces.filter((p) => p.shape === "chevron");
    assert.equal(chevrons.length, 2);
    for (const c of chevrons) {
      assert.ok(c.w >= 2.0, "chevron arms read at gameplay distance");
    }
    // The arms share one apex; the wedge centroid sits on the spawn point.
    const ends = chevrons.map((c) => {
      const ex = (Math.cos(c.rotY) * c.w) / 2;
      const ez = (-Math.sin(c.rotY) * c.w) / 2;
      return [
        [c.dx + ex, c.dz + ez],
        [c.dx - ex, c.dz - ez],
      ];
    });
    let best = Infinity;
    let apex = ends[0][0];
    let far: number[][] = [ends[0][1], ends[1][0]];
    for (const a of ends[0]) {
      for (const b of ends[1]) {
        const gap = Math.hypot(a[0] - b[0], a[1] - b[1]);
        if (gap < best) {
          best = gap;
          apex = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
          far = [
            a === ends[0][0] ? ends[0][1] : ends[0][0],
            b === ends[1][0] ? ends[1][1] : ends[1][0],
          ];
        }
      }
    }
    assert.ok(best <= 0.05, "chevron arms meet at a shared apex");
    const centroid = [(apex[0] + far[0][0] + far[1][0]) / 3, (apex[1] + far[0][1] + far[1][1]) / 3];
    assert.ok(Math.hypot(centroid[0], centroid[1]) <= 0.05, "wedge centers on the pad");
  }
  // Point symmetry through the arena center keeps deployment fair. A half turn
  // maps orientations to themselves plus pi, which the box arms share. Rounding
  // absorbs float dust and signed zeroes from the trig coordinates.
  const num = (v: number) => {
    const q = Math.round(v * 1000) / 1000;
    return (q === 0 ? 0 : q).toFixed(3);
  };
  const turn = (r: number) => num(((r % Math.PI) + Math.PI) % Math.PI);
  const key = (p: SpawnPadPiece) => [p.shape, num(p.dx), num(p.dz), turn(p.rotY)].join("|");
  const mirror = (p: SpawnPadPiece): string => key({ ...p, dx: -p.dx, dz: -p.dz });
  assert.deepEqual(pads[1].map(key).sort(), pads[0].map(mirror).sort());
});
