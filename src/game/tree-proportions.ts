import { Random } from "./math";
import type { Cover } from "./types";

/** Shared deterministic proportions for the standing tree, fallen trunk and rooted stump. */
export function treeProportions(c: Pick<Cover, "x" | "z" | "w" | "d" | "h">) {
  const seed = ((Math.round(c.x * 100) * 73856093) ^ (Math.round(c.z * 100) * 19349663)) >>> 0;
  const rng = new Random(seed);
  const family = Math.floor(rng.next() * 6);
  const twist = rng.range(0, Math.PI * 2);
  const height = c.h * rng.range(0.9, 1.07);
  const radius = Math.min(c.w, c.d) * (family === 3 ? 0.14 : family >= 4 ? 0.1 : 0.12);
  const stumpHeight = radius * rng.range(1.5, 1.9);
  // Block the solid flared trunk, not the thin roots extending along the ground.
  return { seed, rng, family, twist, height, radius, stumpHeight, stumpRadius: radius * 1.25 };
}
