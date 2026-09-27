import { ARENA } from "./data";
import type { Cover, Vec2 } from "./types";
import { treeProportions } from "./tree-proportions";
const CELL_SIZE = 1.5;
const HALF_ARENA = ARENA;
const GRID_SIZE = Math.ceil((2 * HALF_ARENA) / CELL_SIZE);
const CELLS = GRID_SIZE * GRID_SIZE;
// Inflate obstacles by a hull margin so a point path leaves room for the actual tank.
const HULL_CLEARANCE = 1.35;
const REBUILD_PADDING = 2;
const NEAREST_SEARCH_RADIUS = 9;
const DIRECTIONS = [
  [1, 0],
  [-1, 0],
  [0, 1],
  [0, -1],
] as const;
export class Navigation {
  blocked = new Uint8Array(CELLS);
  private costs = new Float32Array(CELLS);
  private parent = new Int32Array(CELLS);
  private closed = new Uint8Array(CELLS);
  /** Discovery order of each cell in the current search, and the cell at each order. */
  private order = new Int32Array(CELLS);
  private discovered = new Int32Array(CELLS);
  /** Min-heap of `estimate * CELLS + discovery order`; see find. */
  private open: number[] = [];
  version = 0;
  paths = 0;
  rebuild(covers: Cover[], region?: Cover): void {
    const obstacles = covers.flatMap((cover) => {
      if (cover.alive) {
        return [cover];
      }
      if (cover.kind === "tree") {
        const diameter = treeProportions(cover).stumpRadius * 2;
        return [{ ...cover, w: diameter, d: diameter }];
      }
      return [];
    });
    const x0 = region
      ? Math.max(
          0,
          Math.floor((region.x - region.w / 2 - REBUILD_PADDING + HALF_ARENA) / CELL_SIZE),
        )
      : 0;
    const x1 = region
      ? Math.min(
          GRID_SIZE - 1,
          Math.ceil((region.x + region.w / 2 + REBUILD_PADDING + HALF_ARENA) / CELL_SIZE),
        )
      : GRID_SIZE - 1;
    const z0 = region
      ? Math.max(
          0,
          Math.floor((region.z - region.d / 2 - REBUILD_PADDING + HALF_ARENA) / CELL_SIZE),
        )
      : 0;
    const z1 = region
      ? Math.min(
          GRID_SIZE - 1,
          Math.ceil((region.z + region.d / 2 + REBUILD_PADDING + HALF_ARENA) / CELL_SIZE),
        )
      : GRID_SIZE - 1;
    for (let z = z0; z <= z1; z++) {
      for (let x = x0; x <= x1; x++) {
        const position = this.point(z * GRID_SIZE + x);
        this.blocked[z * GRID_SIZE + x] = obstacles.some(
          (cover) =>
            Math.abs(position.x - cover.x) < cover.w / 2 + HULL_CLEARANCE &&
            Math.abs(position.z - cover.z) < cover.d / 2 + HULL_CLEARANCE,
        )
          ? 1
          : 0;
      }
    }
    this.version++;
  }
  index(position: Vec2): number {
    return cellAt(position.x, position.z);
  }
  point(i: number): Vec2 {
    return {
      x: ((i % GRID_SIZE) + 0.5) * CELL_SIZE - HALF_ARENA,
      z: (Math.floor(i / GRID_SIZE) + 0.5) * CELL_SIZE - HALF_ARENA,
    };
  }
  nearest(i: number): number {
    if (!this.blocked[i]) {
      return i;
    }
    for (let r = 1; r < NEAREST_SEARCH_RADIUS; r++) {
      for (let z = -r; z <= r; z++) {
        for (let x = -r; x <= r; x++) {
          const a = (i % GRID_SIZE) + x;
          const b = Math.floor(i / GRID_SIZE) + z;
          if (
            a >= 0 &&
            a < GRID_SIZE &&
            b >= 0 &&
            b < GRID_SIZE &&
            !this.blocked[b * GRID_SIZE + a]
          ) {
            return b * GRID_SIZE + a;
          }
        }
      }
    }
    return i;
  }
  /** Conservative grid visibility for shortening routes without cutting corners. */
  clearLine(from: Vec2, to: Vec2): boolean {
    const steps = Math.ceil(Math.hypot(to.x - from.x, to.z - from.z) / (CELL_SIZE / 3));
    for (let i = 0; i <= steps; i++) {
      const f = steps ? i / steps : 0;
      if (this.blocked[cellAt(from.x + (to.x - from.x) * f, from.z + (to.z - from.z) * f)]) {
        return false;
      }
    }
    return true;
  }
  /**
   * Four-neighbor A*: Manhattan distance is admissible. The open list is a heap ordered by
   * estimate, then by the order cells were first reached, which is the order a linear scan
   * of an append-only list picks among equal estimates; seeded routes depend on that order.
   */
  find(from: Vec2, to: Vec2): Vec2[] {
    this.paths++;
    const start = this.nearest(this.index(from));
    const goal = this.nearest(this.index(to));
    const { costs, parent, closed, order, discovered, open } = this;
    costs.fill(Infinity);
    parent.fill(-1);
    closed.fill(0);
    open.length = 0;
    const goalX = goal % GRID_SIZE;
    const goalZ = Math.floor(goal / GRID_SIZE);
    const heuristic = (i: number) =>
      Math.abs((i % GRID_SIZE) - goalX) + Math.abs(Math.floor(i / GRID_SIZE) - goalZ);
    // A cheaper route to an open cell pushes a smaller estimate with the cell's original
    // discovery order, so the outdated entry always pops after the cell has closed.
    let discoveries = 0;
    const reach = (cell: number, cost: number) => {
      if (costs[cell] === Infinity) {
        order[cell] = discoveries;
        discovered[discoveries++] = cell;
      }
      costs[cell] = cost;
      pushHeap(open, (cost + heuristic(cell)) * CELLS + order[cell]);
    };
    reach(start, 0);
    let reached = start;
    while (open.length) {
      const current = discovered[popHeap(open) % CELLS];
      if (closed[current]) {
        continue;
      }
      closed[current] = 1;
      if (current === goal) {
        reached = current;
        break;
      }
      const x = current % GRID_SIZE;
      const z = Math.floor(current / GRID_SIZE);
      for (const [dx, dz] of DIRECTIONS) {
        const nx = x + dx;
        const nz = z + dz;
        if (nx < 0 || nz < 0 || nx >= GRID_SIZE || nz >= GRID_SIZE) {
          continue;
        }
        const ni = nz * GRID_SIZE + nx;
        if (this.blocked[ni] || closed[ni]) {
          continue;
        }
        const cost = costs[current] + 1;
        if (cost < costs[ni]) {
          parent[ni] = current;
          reach(ni, cost);
        }
      }
    }
    if (reached !== goal) {
      return [];
    }
    const path: Vec2[] = [];
    while (reached !== start) {
      path.push(this.point(reached));
      reached = parent[reached];
    }
    return path.reverse();
  }
}

function cellAt(x: number, z: number): number {
  return (
    Math.max(0, Math.min(GRID_SIZE - 1, Math.floor((z + HALF_ARENA) / CELL_SIZE))) * GRID_SIZE +
    Math.max(0, Math.min(GRID_SIZE - 1, Math.floor((x + HALF_ARENA) / CELL_SIZE)))
  );
}

function pushHeap(heap: number[], key: number): void {
  let i = heap.length;
  heap.push(key);
  while (i > 0) {
    const up = (i - 1) >> 1;
    if (heap[up] <= key) {
      break;
    }
    heap[i] = heap[up];
    i = up;
  }
  heap[i] = key;
}

function popHeap(heap: number[]): number {
  const top = heap[0];
  const last = heap.pop()!;
  if (heap.length) {
    let i = 0;
    for (;;) {
      let child = 2 * i + 1;
      if (child >= heap.length) {
        break;
      }
      if (child + 1 < heap.length && heap[child + 1] < heap[child]) {
        child++;
      }
      if (heap[child] >= last) {
        break;
      }
      heap[i] = heap[child];
      i = child;
    }
    heap[i] = last;
  }
  return top;
}
