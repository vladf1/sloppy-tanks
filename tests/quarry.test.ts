import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import type { BufferGeometry } from "three";
import {
  quarryButteFootprint,
  quarryButteSpot,
  quarryScreeSpots,
  quarryStockpileGeometry,
  quarryStockpileSpot,
  quarryTalusGeometry,
  quarryTalusStrips,
} from "../src/game/quarry-benches";
import { quarryScreeGeometry, quarryScreeRubble } from "../src/game/quarry-scree";
import { ACCUM_CELLS, bakeQuarrySoil, QUARRY_SOIL_SIZE } from "../src/game/quarry-soil";
import { QUARRY_DUST_CAPACITY, QUARRY_DUST_MAX_OPACITY, QuarryDust } from "../src/game/quarry-dust";
import { DUST_OPACITY } from "../src/game/effect-materials";
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

test("the central crossing is open and the crate cut opens to tanks only after destruction; barriers and rock survive", () => {
  const sim = quarry();
  try {
    assert.equal(
      sim.nav.clearLine({ x: -48, z: 0 }, { x: 48, z: 0 }),
      true,
      "central crossing remains open",
    );
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
    sim.dispose();
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
    sim.dispose();
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

test("scree, rubble, talus, stockpile and the sentinel butte stay outside the playable boundary", () => {
  const outside = (geometry: BufferGeometry, limit: number, label: string) => {
    const positions = geometry.getAttribute("position");
    for (let i = 0; i < positions.count; i++) {
      const extent = Math.max(Math.abs(positions.getX(i)), Math.abs(positions.getZ(i)));
      assert.ok(extent > limit, `${label} reaches into the arena at ${extent.toFixed(1)}`);
    }
  };
  const spots = quarryScreeSpots();
  assert.ok(spots.length >= 4, "collapses interrupt several terraces");
  for (const spot of spots) {
    const mound = quarryScreeGeometry(spot);
    const rubble = quarryScreeRubble(spot);
    try {
      assert.ok(
        rubble.getAttribute("position").count / 3 < 7500,
        "rubble triangle count is bounded",
      );
      for (const geometry of [mound, rubble]) {
        geometry.rotateY(spot.rotY);
        geometry.translate(spot.x, 0, spot.z);
        // Even loose fragments leave the arena and the haul lane clear.
        outside(geometry, 68, "scree");
      }
    } finally {
      mound.dispose();
      rubble.dispose();
    }
  }
  const strips = quarryTalusStrips();
  assert.equal(strips.length, 4, "every lowest cut has a talus toe");
  for (const strip of strips) {
    const talus = quarryTalusGeometry(strip.x0, strip.x1, strip.seed);
    try {
      talus.rotateY(strip.rotY);
      talus.translate(strip.x, 0, strip.z);
      outside(talus, 70, "talus");
    } finally {
      talus.dispose();
    }
  }
  const pile = quarryStockpileSpot();
  const stockpile = quarryStockpileGeometry(pile);
  try {
    stockpile.translate(pile.x, 0, pile.z);
    outside(stockpile, 62, "stockpile toe");
  } finally {
    stockpile.dispose();
  }
  const butte = quarryButteSpot();
  for (const [x, z] of quarryButteFootprint(butte)) {
    assert.ok(
      Math.max(Math.abs(x), Math.abs(z)) > 60.5,
      `butte slab inside the boundary wall: ${x.toFixed(1)},${z.toFixed(1)}`,
    );
  }
});

test("spawn pads stay below cover height, compact and mirrored between teams", () => {
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

test("quarry soil bands bake the same pixels as one continuous pass", () => {
  const accum = Float32Array.from({ length: ACCUM_CELLS * ACCUM_CELLS }, (_, i) => (i % 7) / 10);
  const row = QUARRY_SOIL_SIZE * 4;
  const whole = bakeQuarrySoil(accum, 0, 24);
  const split = [...bakeQuarrySoil(accum, 0, 9), ...bakeQuarrySoil(accum, 9, 24)];
  assert.deepEqual(split, [...whole]);
  // A worker band deep in the image replays the shared stream's earlier draws.
  const late = bakeQuarrySoil(accum, 1200, 1203);
  assert.deepEqual(late, bakeQuarrySoil(accum, 1197, 1203).slice(3 * row));
});

function dustFixture(map: "village" | "quarry" = "quarry") {
  const sim = new Simulation(123);
  sim.mapMode = map;
  sim.reset(2);
  sim.start();
  const dust = new QuarryDust();
  const step = (dt = 1 / 60) => {
    sim.elapsed += dt;
    dust.update(sim, dt);
  };
  const dispose = () => {
    dust.reset();
    sim.dispose();
  };
  return { sim, dust, step, dispose };
}

test("wind wisps stay within the fixed pool, opacity cap and finite poses", () => {
  const { dust, step, dispose } = dustFixture();
  try {
    for (let i = 0; i < 2000; i++) step(1 / 30);
    assert.ok(dust.mesh.count > 0, "sparse wisps accumulate while playing");
    assert.ok(dust.mesh.count <= QUARRY_DUST_CAPACITY);
    const opacity = dust.mesh.geometry.getAttribute(DUST_OPACITY);
    const matrices = dust.mesh.instanceMatrix.array;
    for (let i = 0; i < dust.mesh.count; i++) {
      assert.ok(opacity.getX(i) <= QUARRY_DUST_MAX_OPACITY + 1e-6);
      for (let k = 0; k < 16; k++) assert.ok(Number.isFinite(matrices[i * 16 + k]));
    }
  } finally {
    dispose();
  }
});

test("wind dust freezes while paused, clears on reset and stops when the map changes", () => {
  const quarry = dustFixture("quarry");
  const village = dustFixture("village");
  try {
    const { sim, dust, step } = quarry;
    for (let i = 0; i < 60; i++) step();
    assert.ok(dust.mesh.count > 0);
    assert.equal(dust.mesh.visible, true);
    const matrices = dust.mesh.instanceMatrix.array.slice();
    sim.match.phase = "paused";
    for (let i = 0; i < 30; i++) step();
    assert.deepEqual(dust.mesh.instanceMatrix.array, matrices);
    sim.match.phase = "playing";
    dust.reset();
    assert.equal(dust.mesh.count, 0);
    assert.equal(dust.mesh.visible, false);
    for (let i = 0; i < 60; i++) step();
    assert.ok(dust.mesh.count > 0);
    for (let i = 0; i < 60; i++) village.step();
    assert.equal(village.dust.mesh.count, 0);
    assert.equal(village.dust.mesh.visible, false);
    // A stale quarry pool clears itself the moment the theme changes.
    sim.mapMode = "village";
    sim.reset(2);
    sim.start();
    step();
    assert.equal(dust.mesh.count, 0);
    assert.equal(dust.mesh.visible, false);
  } finally {
    quarry.dispose();
    village.dispose();
  }
});
