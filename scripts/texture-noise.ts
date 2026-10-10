import { Random } from "./asset-data";

// Seeded value noise and blend helpers shared by the procedural texture generators.

const wrap = (i: number, period: number) => (period ? ((i % period) + period) % period : i);

/** Value noise on a lattice that wraps every `period` cells in x (0 = no wrap), and in y
 * as well with `wrapY`. */
export function lattice(seed: number, wrapY = false) {
  const table = new Float32Array(4096);
  const local = new Random(seed);
  for (let i = 0; i < table.length; i++) table[i] = local.next();
  const at = (ix: number, iy: number, period: number) => {
    const x = wrap(ix, period);
    const y = wrapY ? wrap(iy, period) : iy;
    return table[((x * 73856093) ^ (y * 19349663)) & 4095];
  };
  return (x: number, y: number, period = 0) => {
    const ix = Math.floor(x);
    const iy = Math.floor(y);
    const fx = x - ix;
    const fy = y - iy;
    const sx = fx * fx * (3 - 2 * fx);
    const sy = fy * fy * (3 - 2 * fy);
    const a = at(ix, iy, period);
    const b = at(ix + 1, iy, period);
    const c = at(ix, iy + 1, period);
    const d = at(ix + 1, iy + 1, period);
    return a + (b - a) * sx + (c - a) * sy + (a - b - c + d) * sx * sy;
  };
}

/** Fractal noise whose period is `period` lattice cells at the first octave. */
export function fbm(
  noise: ReturnType<typeof lattice>,
  x: number,
  y: number,
  period: number,
  octaves = 4,
) {
  let sum = 0;
  let amplitude = 0.5;
  let scale = 1;
  for (let i = 0; i < octaves; i++) {
    sum += amplitude * noise(x * scale, y * scale, period * scale);
    amplitude *= 0.5;
    scale *= 2;
  }
  return sum;
}

export const mix = (a: number, b: number, t: number) => a + (b - a) * t;
export const smooth = (e0: number, e1: number, x: number) => {
  const t = Math.min(1, Math.max(0, (x - e0) / (e1 - e0)));
  return t * t * (3 - 2 * t);
};
