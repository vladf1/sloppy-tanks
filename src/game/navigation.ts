import { ARENA } from "./data";
import type { Cover, Vec2 } from "./types";
const CELL_SIZE = 1.5;
const HALF_ARENA = ARENA;
const GRID_SIZE = Math.ceil((2 * HALF_ARENA) / CELL_SIZE);
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
  blocked = new Uint8Array(GRID_SIZE * GRID_SIZE);
  private costs = new Float32Array(GRID_SIZE * GRID_SIZE);
  private parent = new Int32Array(GRID_SIZE * GRID_SIZE);
  private closed = new Uint8Array(GRID_SIZE * GRID_SIZE);
  private open: number[] = [];
  version = 0;
  paths = 0;
  rebuild(covers: Cover[], region?: Cover): void {
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
        this.blocked[z * GRID_SIZE + x] = covers.some(
          (cover) =>
            cover.alive &&
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
    return (
      Math.max(0, Math.min(GRID_SIZE - 1, Math.floor((position.z + HALF_ARENA) / CELL_SIZE))) *
        GRID_SIZE +
      Math.max(0, Math.min(GRID_SIZE - 1, Math.floor((position.x + HALF_ARENA) / CELL_SIZE)))
    );
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
      if (
        this.blocked[
          this.index({ x: from.x + (to.x - from.x) * f, z: from.z + (to.z - from.z) * f })
        ]
      ) {
        return false;
      }
    }
    return true;
  }
  /** Four-neighbor A*: Manhattan distance is admissible, and ties preserve insertion order. */
  find(from: Vec2, to: Vec2): Vec2[] {
    this.paths++;
    const start = this.nearest(this.index(from));
    const goal = this.nearest(this.index(to));
    const { costs, parent, closed, open } = this;
    costs.fill(Infinity);
    parent.fill(-1);
    closed.fill(0);
    open.length = 0;
    open.push(start);
    costs[start] = 0;
    const heuristic = (i: number) =>
      Math.abs((i % GRID_SIZE) - (goal % GRID_SIZE)) +
      Math.abs(Math.floor(i / GRID_SIZE) - Math.floor(goal / GRID_SIZE));
    let reached = start;
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
          costs[ni] = cost;
          parent[ni] = current;
          open.push(ni);
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
