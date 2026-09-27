import { test } from "node:test";
import assert from "node:assert/strict";
import { Random } from "../src/game/data";
import { Navigation } from "../src/game/navigation";
import type { Vec2 } from "../src/game/types";

/** The original linear-scan A*. Seeded bot routes depend on its tie order: among equal
 * estimates it expands the cell that entered the append-only open list first. */
function linearScanRoute(nav: Navigation, from: Vec2, to: Vec2) {
  const size = Math.sqrt(nav.blocked.length);
  const start = nav.nearest(nav.index(from));
  const goal = nav.nearest(nav.index(to));
  const costs = new Float32Array(nav.blocked.length).fill(Infinity);
  const parent = new Int32Array(nav.blocked.length).fill(-1);
  const closed = new Uint8Array(nav.blocked.length);
  const open = [start];
  costs[start] = 0;
  const heuristic = (i: number) =>
    Math.abs((i % size) - (goal % size)) + Math.abs(Math.floor(i / size) - Math.floor(goal / size));
  let reached = start;
  let reopened = 0;
  while (open.length) {
    let best = 0;
    for (let i = 1; i < open.length; i++) {
      if (costs[open[i]] + heuristic(open[i]) < costs[open[best]] + heuristic(open[best])) {
        best = i;
      }
    }
    const current = open.splice(best, 1)[0];
    if (closed[current]) {
      continue;
    }
    closed[current] = 1;
    if (current === goal) {
      reached = current;
      break;
    }
    const x = current % size;
    const z = Math.floor(current / size);
    for (const [dx, dz] of [
      [1, 0],
      [-1, 0],
      [0, 1],
      [0, -1],
    ]) {
      const nx = x + dx;
      const nz = z + dz;
      if (nx < 0 || nz < 0 || nx >= size || nz >= size) {
        continue;
      }
      const ni = nz * size + nx;
      if (nav.blocked[ni] || closed[ni]) {
        continue;
      }
      const cost = costs[current] + 1;
      if (cost < costs[ni]) {
        reopened += costs[ni] === Infinity ? 0 : 1;
        costs[ni] = cost;
        parent[ni] = current;
        open.push(ni);
      }
    }
  }
  const path: Vec2[] = [];
  if (reached !== goal) {
    return { path, reopened };
  }
  while (reached !== start) {
    path.push(nav.point(reached));
    reached = parent[reached];
  }
  return { path: path.reverse(), reopened };
}

test("heap A* returns exactly the routes of the linear-scan search it replaced", () => {
  const random = new Random(9001);
  const nav = new Navigation();
  const size = Math.sqrt(nav.blocked.length);
  const point = () => ({ x: random.range(-62, 62), z: random.range(-62, 62) });
  let reopened = 0;
  let unreachable = 0;
  for (let layout = 0; layout < 12; layout++) {
    // Open ground has many equal-cost routes; scattered blocks and long walls force the
    // search to reopen cells through cheaper detours.
    nav.blocked.fill(0);
    const density = 0.05 + layout * 0.03;
    for (let i = 0; i < nav.blocked.length; i++) {
      nav.blocked[i] = random.next() < density ? 1 : 0;
    }
    for (let wall = 0; wall < layout; wall++) {
      const row = Math.floor(random.range(2, size - 2));
      const gap = Math.floor(random.range(0, size));
      for (let x = 0; x < size; x++) {
        if (Math.abs(x - gap) > 1) {
          nav.blocked[wall % 2 ? x * size + row : row * size + x] = 1;
        }
      }
    }
    for (let search = 0; search < 25; search++) {
      const from = point();
      const to = point();
      const expected = linearScanRoute(nav, from, to);
      assert.deepEqual(nav.find(from, to), expected.path, `layout ${layout}, search ${search}`);
      reopened += expected.reopened;
      unreachable += expected.path.length ? 0 : 1;
    }
  }
  // Both cases exercise the heap's outdated entries and a fully drained open list.
  assert.ok(reopened > 0, "some searches find a cheaper route to an open cell");
  assert.ok(unreachable > 0, "some goals are walled off");
});
