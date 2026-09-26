import { test } from "node:test";
import assert from "node:assert/strict";
import { pickupLayout, spawnPositions } from "../src/game/arena";
import { isSpecialAmmo } from "../src/game/ammunition";
import { distance } from "../src/game/data";
import { MAPS } from "../src/game/maps";
import { Navigation } from "../src/game/navigation";
import type { Cover, Vec2 } from "../src/game/types";

/** Clear floor a tank hull needs around a spawn, pickup or flank waypoint. */
const HULL_CLEARANCE = 2;
/** A route ends in the navigation cell that contains its goal. */
const ARRIVAL = 1.8;
/** Open lanes along both far edges that each map must keep reachable. */
const FLANKS = [-52, 52].flatMap((z) => [-45, 0, 45].map((x) => ({ x, z })));
const spawns = [...spawnPositions(0), ...spawnPositions(1)];

test("pickups and spawn slots are point-symmetric; ammo and repairs sit away from spawn pads", () => {
  for (const p of pickupLayout)
    assert.ok(
      pickupLayout.some((o) => o.kind === p.kind && o.x === -p.x && o.z === -p.z),
      JSON.stringify(p),
    );
  const [a, b] = [spawnPositions(0), spawnPositions(1)];
  assert.equal(a.length, b.length);
  for (let i = 0; i < a.length; i++) assert.deepEqual([a[i].x, a[i].z], [-b[i].x, -b[i].z]);
  const ammo = pickupLayout.filter((p) => isSpecialAmmo(p.kind));
  assert.equal(ammo.length, 8);
  for (const kind of new Set(ammo.map((p) => p.kind)))
    assert.equal(ammo.filter((p) => p.kind === kind).length, 2, kind);
  for (const p of ammo) assert.ok(Math.hypot(p.x, p.z) > 20, "ammo is contested away from center");
  const repairs = pickupLayout.filter((p) => p.kind === "repair");
  assert.equal(repairs.length, 4);
  for (const p of repairs)
    for (const spawn of spawns)
      assert.ok(distance(p, spawn) >= 8, `repair at ${p.x},${p.z} is too close to a spawn pad`);
});

for (const map of MAPS)
  test(`${map.name} cover is point-symmetric and leaves every spawn, pickup and flank clear and reachable`, () => {
    const layout = map.layout();
    for (const c of layout)
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
    const nav = new Navigation();
    nav.rebuild(layout.map((c) => ({ ...c, alive: true })) as Cover[]);
    const points: Vec2[] = [...spawns, ...pickupLayout, ...FLANKS];
    for (const point of points) {
      const label = JSON.stringify({ x: point.x, z: point.z });
      for (const c of layout)
        assert.ok(
          Math.abs(point.x - c.x) >= c.w / 2 + HULL_CLEARANCE ||
            Math.abs(point.z - c.z) >= c.d / 2 + HULL_CLEARANCE,
          `hull clearance ${label} / ${c.kind} at ${c.x},${c.z}`,
        );
      assert.equal(nav.blocked[nav.index(point)], 0, `blocked ${label}`);
      // Both teams' spawn lines must reach every point, not only the nearer one.
      for (const start of [spawnPositions(0)[0], spawnPositions(1)[0]]) {
        if (start.x === point.x && start.z === point.z) continue;
        const end = nav.find(start, point).at(-1);
        assert.ok(end && distance(end, point) < ARRIVAL, `unreachable ${label}`);
      }
    }
  });
