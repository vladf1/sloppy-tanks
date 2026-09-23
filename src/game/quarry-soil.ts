import { Random } from "./math";

// Pure soil bake shared by the page and its bake workers: no DOM, Three.js or
// layout imports. Local smoothstep/lerp copy Three's MathUtils arithmetic exactly.
export const QUARRY_TERRAIN_EXTENT = 210;
export const QUARRY_SOIL_SIZE = 2048;
export const ACCUM_CELLS = 105;
const EXTENT = QUARRY_TERRAIN_EXTENT;

const smooth = (x: number, min: number, max: number) => {
  if (x <= min) {
    return 0;
  }
  if (x >= max) {
    return 1;
  }
  x = (x - min) / (max - min);
  return x * x * (3 - 2 * x);
};
const lerp = (x: number, y: number, t: number) => (1 - t) * x + t * y;

const noiseSeed = new Random(38012);
const soilNoise = Float32Array.from({ length: 256 * 256 }, () => noiseSeed.next());

/** Smooth deterministic value noise, shared by the macro soil and fine aggregate. */
function noise(x: number, z: number): number {
  const ix = Math.floor(x);
  const iz = Math.floor(z);
  const hash = (a: number, b: number) => soilNoise[(a & 255) + (b & 255) * 256];
  const fx = x - ix;
  const fz = z - iz;
  const u = fx * fx * (3 - 2 * fx);
  const v = fz * fz * (3 - 2 * fz);
  return lerp(
    lerp(hash(ix, iz), hash(ix + 1, iz), u),
    lerp(hash(ix, iz + 1), hash(ix + 1, iz + 1), u),
    v,
  );
}

function sampleAccum(grid: Float32Array, x: number, z: number): number {
  const cell = EXTENT / (ACCUM_CELLS - 1);
  const fx = Math.max(0, Math.min(ACCUM_CELLS - 1.001, (x + EXTENT / 2) / cell));
  const fz = Math.max(0, Math.min(ACCUM_CELLS - 1.001, (z + EXTENT / 2) / cell));
  const i = Math.floor(fx);
  const j = Math.floor(fz);
  const u = fx - i;
  const v = fz - j;
  return (
    grid[j * ACCUM_CELLS + i] * (1 - u) * (1 - v) +
    grid[j * ACCUM_CELLS + i + 1] * u * (1 - v) +
    grid[(j + 1) * ACCUM_CELLS + i] * (1 - u) * v +
    grid[(j + 1) * ACCUM_CELLS + i + 1] * u * v
  );
}

/** Bake sRGBA rows [start, end) of the work-yard soil. Any row split produces
 * the same pixels: each pixel draws two or three values from one seeded stream,
 * so a later band first replays the earlier draws. */
export function bakeQuarrySoil(
  accum: Float32Array,
  start = 0,
  end = QUARRY_SOIL_SIZE,
): Uint8ClampedArray<ArrayBuffer> {
  const size = QUARRY_SOIL_SIZE;
  const extent = EXTENT;
  const pixels = new Uint8ClampedArray((end - start) * size * 4);
  const rng = new Random(7391);
  for (let skipped = start * size; skipped > 0; skipped--) {
    rng.next();
    if (rng.next() > 0.975) {
      rng.next();
    }
  }
  for (let row = start; row < end; row++) {
    const z = (row / (size - 1) - 0.5) * extent;
    for (let col = 0; col < size; col++) {
      const x = (col / (size - 1) - 0.5) * extent;
      const macro = noise(x * 0.065, z * 0.065);
      const grit = noise(x * 0.75, z * 0.75);
      // Broad irregular sheets: pale windblown sand where the macro field runs
      // high, exposed rocky soil where it runs low. A sine warp plus two fixed
      // diagonal drift bands break any hint of cellular repetition without
      // adding another noise octave to the bake.
      const warp = Math.sin(x * 0.021 + 1.7) * Math.sin(z * 0.023 - 0.6);
      const bands = Math.sin(x * 0.045 + z * 0.031 + 1.2) * Math.sin(z * 0.052 - x * 0.013 + 0.4);
      const sand = smooth(macro + warp * 0.18 + bands * 0.1, 0.5, 0.74);
      const rocky = 1 - smooth(macro - warp * 0.15, 0.24, 0.5);
      // Rounded outer haul loop and a gently wandering east/west crossing.
      const qx = Math.abs(x) - 39;
      const qz = Math.abs(z) - 39;
      const loop = Math.abs(
        Math.hypot(Math.max(qx, 0), Math.max(qz, 0)) + Math.min(Math.max(qx, qz), 0) - 12,
      );
      const crossing = Math.abs(z - Math.sin(x * 0.055) * 1.6);
      const distance = Math.min(loop, crossing);
      const edge = (noise(x * 0.42, z * 0.42) - 0.5) * 1.6;
      const road = 1 - smooth(distance + edge, 3.4, 8.0);
      const rut = Math.exp(-Math.pow((distance - 2.25) / 0.38, 2)) * road;
      // Trampled work floor: one broad central apron plus two midfield patches
      // between the rock shoulders. Fixed smooth shapes, no extra noise.
      const wear = Math.max(
        1 - smooth(Math.hypot(x / 30, z / 23), 0.55, 1),
        1 - smooth(Math.hypot((Math.abs(x) - 25) / 13, z / 16), 0.5, 1),
      );
      const fine = rng.range(-5, 5);
      const aggregate = rng.next() > 0.975 ? rng.range(-22, 17) : 0;
      // sRGB bytes: mid soil -> pale sand -> rocky soil -> dark compacted road.
      let r = 178;
      let g = 150;
      let b = 114;
      const wornSand = sand * (1 - wear * 0.55);
      r += (230 - r) * wornSand;
      g += (208 - g) * wornSand;
      b += (168 - b) * wornSand;
      const offRoad = 1 - road;
      r += (169 - r) * rocky * offRoad;
      g += (122 - g) * rocky * offRoad;
      b += (83 - b) * rocky * offRoad;
      r += (129 - r) * road;
      g += (104 - g) * road;
      b += (79 - b) * road;
      r += (152 - r) * wear * 0.5 * offRoad;
      g += (124 - g) * wear * 0.5 * offRoad;
      b += (96 - b) * wear * 0.5 * offRoad;
      const shade = (macro - 0.5) * 8 + (grit - 0.5) * 15 + fine + aggregate - rut * 13;
      // Drifted sand against cover bases and along the quiet outer shoulders.
      const shoulder = smooth(Math.max(Math.abs(x), Math.abs(z)), 48, 58) * offRoad;
      const drift = Math.min(0.5, sampleAccum(accum, x, z) * 0.55 + shoulder * 0.3);
      r += (226 - r) * drift;
      g += (209 - g) * drift;
      b += (176 - b) * drift;
      const i = ((row - start) * size + col) * 4;
      pixels[i] = r + shade;
      pixels[i + 1] = g + shade;
      pixels[i + 2] = b + shade;
      pixels[i + 3] = 255;
    }
  }
  return pixels;
}
