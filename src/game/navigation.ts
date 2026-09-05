import type { Cover, Vec2 } from "./types";
import { ARENA } from "./data";
const CELL = 1.5,
  HALF = ARENA,
  N = Math.ceil((2 * HALF) / CELL);
export class Navigation {
  blocked = new Uint8Array(N * N);
  version = 0;
  paths = 0;
  rebuild(covers: Cover[], region?: Cover) {
    const x0 = region
      ? Math.max(0, Math.floor((region.x - region.w / 2 - 2 + HALF) / CELL))
      : 0;
    const x1 = region
      ? Math.min(N - 1, Math.ceil((region.x + region.w / 2 + 2 + HALF) / CELL))
      : N - 1;
    const z0 = region
      ? Math.max(0, Math.floor((region.z - region.d / 2 - 2 + HALF) / CELL))
      : 0;
    const z1 = region
      ? Math.min(N - 1, Math.ceil((region.z + region.d / 2 + 2 + HALF) / CELL))
      : N - 1;
    for (let z = z0; z <= z1; z++)
      for (let x = x0; x <= x1; x++) {
        const p = this.point(z * N + x);
        this.blocked[z * N + x] = covers.some(
          (c) =>
            c.alive &&
            Math.abs(p.x - c.x) < c.w / 2 + 1.35 &&
            Math.abs(p.z - c.z) < c.d / 2 + 1.35,
        )
          ? 1
          : 0;
      }
    this.version++;
  }
  index(p: Vec2) {
    return (
      Math.max(0, Math.min(N - 1, Math.floor((p.z + HALF) / CELL))) * N +
      Math.max(0, Math.min(N - 1, Math.floor((p.x + HALF) / CELL)))
    );
  }
  point(i: number): Vec2 {
    return {
      x: ((i % N) + 0.5) * CELL - HALF,
      z: (Math.floor(i / N) + 0.5) * CELL - HALF,
    };
  }
  nearest(i: number) {
    if (!this.blocked[i]) return i;
    for (let r = 1; r < 9; r++)
      for (let z = -r; z <= r; z++)
        for (let x = -r; x <= r; x++) {
          const a = (i % N) + x,
            b = Math.floor(i / N) + z;
          if (a >= 0 && a < N && b >= 0 && b < N && !this.blocked[b * N + a])
            return b * N + a;
        }
    return i;
  }
  find(from: Vec2, to: Vec2): Vec2[] {
    this.paths++;
    const start = this.nearest(this.index(from)),
      goal = this.nearest(this.index(to));
    const costs = new Float32Array(N * N).fill(Infinity),
      parent = new Int32Array(N * N).fill(-1),
      closed = new Uint8Array(N * N);
    const open = [start];
    costs[start] = 0;
    const heuristic = (i: number) =>
      Math.abs((i % N) - (goal % N)) +
      Math.abs(Math.floor(i / N) - Math.floor(goal / N));
    let reached = start;
    while (open.length) {
      let best = 0;
      for (let i = 1; i < open.length; i++)
        if (
          costs[open[i]] + heuristic(open[i]) <
          costs[open[best]] + heuristic(open[best])
        )
          best = i;
      const current = open.splice(best, 1)[0];
      if (closed[current]) continue;
      closed[current] = 1;
      if (current === goal) {
        reached = current;
        break;
      }
      const x = current % N,
        z = Math.floor(current / N);
      for (const [dx, dz] of [
        [1, 0],
        [-1, 0],
        [0, 1],
        [0, -1],
      ]) {
        const nx = x + dx,
          nz = z + dz;
        if (nx < 0 || nz < 0 || nx >= N || nz >= N) continue;
        const ni = nz * N + nx;
        if (this.blocked[ni] || closed[ni]) continue;
        const cost = costs[current] + 1;
        if (cost < costs[ni]) {
          costs[ni] = cost;
          parent[ni] = current;
          open.push(ni);
        }
      }
    }
    if (reached !== goal) return [];
    const path: Vec2[] = [];
    while (reached !== start) {
      path.push(this.point(reached));
      reached = parent[reached];
    }
    return path.reverse();
  }
}
