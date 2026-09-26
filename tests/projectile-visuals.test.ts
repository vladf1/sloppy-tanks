import { test } from "node:test";
import assert from "node:assert/strict";
import { Color, Matrix4, Quaternion, Vector3 } from "three";
import { AMMO_ORDER, PROJECTILE_ORDER } from "../src/game/ammunition";
import { TEAM_COLORS } from "../src/game/data";
import { ProjectileVisuals } from "../src/game/projectile-visuals";
import type { RenderShot } from "../src/game/render-state";

const CAPACITY = 600;

function shot(id: number, weapon: RenderShot["weapon"], team: 0 | 1, x = 0, z = 0): RenderShot {
  return { id, weapon, team, x, z, y: 1, vx: 12, vz: -18 };
}

/** Both teams fire every player munition; spread is three pellets. */
function volley(): RenderShot[] {
  const shots: RenderShot[] = [];
  for (const team of [0, 1] as const) {
    for (const [i, weapon] of AMMO_ORDER.entries()) {
      for (let pellet = 0; pellet < (weapon === "spread" ? 3 : 1); pellet++) {
        shots.push(shot(shots.length, weapon, team, (i - 2) * 4 + pellet * 0.4, team * 4 - 5));
      }
    }
  }
  return shots;
}

function layers(visuals: ProjectileVisuals) {
  return Object.values(visuals.batches).flatMap((batch) =>
    [batch.body, batch.team, batch.exhaust].filter((layer) => layer !== undefined),
  );
}

test("every munition draws aligned body, team and exhaust instances for both teams", () => {
  const visuals = new ProjectileVisuals();
  const shots = volley();
  visuals.update(shots, 2);
  assert.deepEqual(
    PROJECTILE_ORDER.map((weapon) => visuals.batches[weapon].body.count),
    [2, 6, 2, 2, 2, 0],
  );
  for (const weapon of PROJECTILE_ORDER) {
    const { body, team, exhaust } = visuals.batches[weapon];
    assert.equal(team.count, body.count, `${weapon} team layer`);
    assert.equal(exhaust?.count ?? body.count, body.count, `${weapon} exhaust layer`);
    assert.equal(exhaust !== undefined, weapon === "rocket" || weapon === "tow", weapon);
  }
  // Instances follow shot order within a batch, so team colors follow the shots' teams.
  const standard = visuals.batches.standard.team;
  const color = new Color();
  for (const [index, team] of [0, 1].entries()) {
    standard.getColorAt(index, color);
    assert.equal(color.getHex(), new Color(TEAM_COLORS[team]).getHex());
  }
});

test("instances sit at the shot position facing its velocity", () => {
  const visuals = new ProjectileVisuals();
  visuals.update([{ ...shot(1, "piercing", 0, 3, -4), visualY: 1.4 }], 0);
  const matrix = new Matrix4();
  visuals.batches.piercing.body.getMatrixAt(0, matrix);
  const position = new Vector3();
  const rotation = new Quaternion();
  matrix.decompose(position, rotation, new Vector3());
  assert.ok(position.distanceTo(new Vector3(3, 1.4, -4)) < 1e-6, "visual height wins over y");
  const forward = new Vector3(0, 0, 1).applyQuaternion(rotation);
  assert.ok(Math.abs(Math.atan2(forward.x, forward.z) - Math.atan2(12, -18)) < 1e-6);
});

test("player munition models stay compact", () => {
  const visuals = new ProjectileVisuals();
  for (const weapon of AMMO_ORDER) {
    const geometry = visuals.batches[weapon].body.geometry;
    geometry.computeBoundingBox();
    const { min, max } = geometry.boundingBox!;
    assert.ok(max.x - min.x <= 0.53, `${weapon} width ${max.x - min.x}`);
    assert.ok(max.z - min.z <= 1.01, `${weapon} length ${max.z - min.z}`);
  }
});

test("a flood of shots is capped with finite transforms and cleared by the next empty update", () => {
  const visuals = new ProjectileVisuals();
  const template = volley();
  const shots = Array.from({ length: 650 }, (_, i) => ({
    ...template[i % template.length],
    id: i,
    x: ((i % 30) - 15) * 1.5,
    z: (Math.floor(i / 30) - 10) * 1.5,
  }));
  visuals.update(shots, 3);
  const drawn = Object.values(visuals.batches).reduce((sum, batch) => sum + batch.body.count, 0);
  assert.equal(drawn, CAPACITY);
  for (const layer of layers(visuals)) {
    const used = layer.instanceMatrix.array.subarray(0, layer.count * 16);
    assert.ok(used.every(Number.isFinite), "finite instance matrices");
  }
  visuals.update([], 4);
  assert.ok(
    layers(visuals).every((layer) => layer.count === 0),
    "no stale instances",
  );
});

test("drawing reads shots without mutating simulation state", () => {
  const visuals = new ProjectileVisuals();
  const shots = volley().map((s) => Object.freeze(s));
  const before = JSON.stringify(shots);
  visuals.update(Object.freeze(shots), 1.5);
  assert.equal(JSON.stringify(shots), before);
});
