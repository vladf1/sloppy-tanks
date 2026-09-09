import type { Vec2 } from "./types";
/** Mulberry32: keep these bit operations and draw order stable for seeded matches. */
export class Random {
  constructor(public state: number) {}
  next(): number {
    let t = (this.state += 0x6d2b79f5);
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }
  range(a: number, b: number): number {
    return a + (b - a) * this.next();
  }
}
export const distance = (a: Vec2, b: Vec2) => Math.hypot(a.x - b.x, a.z - b.z);
export const angleDelta = (a: number, b: number) => Math.atan2(Math.sin(b - a), Math.cos(b - a));

/** Highest score wins; equal scores retain the original candidate order. */
export function bestBy<T>(items: Iterable<T>, score: (item: T) => number): T | undefined {
  let best: T | undefined;
  let highest = -Infinity;
  for (const item of items) {
    const value = score(item);
    if (value > highest) {
      best = item;
      highest = value;
    }
  }
  return best;
}
