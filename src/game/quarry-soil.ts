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
 * so a later band first replays the earlier draws. Alpha carries how gritty the
 * soil is (128 fine sand ... 255 loose gravel) for the world-space detail shader;
 * it never blends, since the terrain material is opaque. */
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
      // Broad irregular sheets: windblown sand where the macro field runs high,
      // exposed stony soil where it runs low. A sine warp plus two fixed diagonal
      // drift bands break any hint of cellular repetition without another octave.
      const warp = Math.sin(x * 0.021 + 1.7) * Math.sin(z * 0.023 - 0.6);
      const bands = Math.sin(x * 0.045 + z * 0.031 + 1.2) * Math.sin(z * 0.052 - x * 0.013 + 0.4);
      const sand = smooth(macro + warp * 0.18 + bands * 0.1, 0.54, 0.68);
      const rocky = 1 - smooth(macro - warp * 0.15, 0.27, 0.45);
      // Rounded outer haul loop and a gently wandering east/west crossing: a
      // graded, compacted bed with crisp shoulders and a windrow of loose spill.
      const qx = Math.abs(x) - 39;
      const qz = Math.abs(z) - 39;
      const loop = Math.abs(
        Math.hypot(Math.max(qx, 0), Math.max(qz, 0)) + Math.min(Math.max(qx, qz), 0) - 12,
      );
      const crossing = Math.abs(z - Math.sin(x * 0.055) * 1.6);
      const distance = Math.min(loop, crossing);
      let road = 0;
      let windrow = 0;
      let rut = 0;
      if (distance < 9.5) {
        const edge = (noise(x * 0.42, z * 0.42) - 0.5) * 1.3;
        road = 1 - smooth(distance + edge, 4.2, 5.5);
        const w = (distance + edge * 0.7 - 6.1) / 0.8;
        windrow = Math.exp(-w * w);
        // Two-lane dual-wheel haul tracks, pressed darker into the compacted bed.
        const inner = (distance - 0.8) / 0.26;
        const outer = (distance - 3.5) / 0.3;
        rut = Math.max(Math.exp(-inner * inner), Math.exp(-outer * outer) * 0.8) * road;
      }
      // Trampled work floor: one broad central apron plus two midfield patches
      // between the rock shoulders. Fixed smooth shapes, no extra noise.
      const wear = Math.max(
        1 - smooth(Math.hypot(x / 30, z / 23), 0.55, 1),
        1 - smooth(Math.hypot((Math.abs(x) - 25) / 13, z / 16), 0.5, 1),
      );
      // Beyond the wall the ground falls to the machinery apron: loose fill on
      // the embankment, then a working floor of darker quarry fines.
      const reach = Math.max(Math.abs(x), Math.abs(z));
      const apron = smooth(reach, 60.2, 61.5);
      const fill = apron * (1 - smooth(reach, 65, 67.5));
      const fine = rng.range(-5, 5) * 0.6;
      const aggregate = rng.next() > 0.975 ? rng.range(-22, 17) * 0.6 : 0;
      // sRGB bytes: buff soil -> pale sand -> stony soil -> compacted grey-tan road.
      let r = 186;
      let g = 165;
      let b = 134;
      const offRoad = 1 - road;
      const wornSand = sand * (1 - wear * 0.55) * (1 - apron * 0.6);
      r += (213 - r) * wornSand;
      g += (195 - g) * wornSand;
      b += (163 - b) * wornSand;
      const stony = Math.min(1, rocky + fill * 0.7) * offRoad;
      r += (168 - r) * stony;
      g += (141 - g) * stony;
      b += (109 - b) * stony;
      r += (170 - r) * road;
      g += (155 - g) * road;
      b += (131 - b) * road;
      r += (198 - r) * windrow * 0.55;
      g += (180 - g) * windrow * 0.55;
      b += (150 - b) * windrow * 0.55;
      r += (171 - r) * wear * 0.45 * offRoad;
      g += (151 - g) * wear * 0.45 * offRoad;
      b += (122 - b) * wear * 0.45 * offRoad;
      // Machinery apron: darker fines, spattered with the same stony patches.
      const working = apron * (1 - fill);
      r += (168 - r) * working * 0.5;
      g += (150 - g) * working * 0.5;
      b += (124 - b) * working * 0.5;
      // Wind ripples: sub-metre crests across the prevailing +x wind on open sand.
      const ripple =
        wornSand * offRoad > 0.02
          ? Math.sin(x * 8.3 + Math.sin(z * 0.9 + x * 0.21) * 2.4 + macro * 9) *
            wornSand *
            offRoad *
            3.5
          : 0;
      const shade = (macro - 0.5) * 9 + (grit - 0.5) * 13 + fine + aggregate - rut * 9 + ripple;
      // Drifted sand against cover bases and along the quiet outer shoulders.
      const shoulder = smooth(reach, 48, 58) * (1 - apron) * offRoad;
      const drift = Math.min(0.5, sampleAccum(accum, x, z) * 0.55 + shoulder * 0.3);
      r += (216 - r) * drift;
      g += (199 - g) * drift;
      b += (167 - b) * drift;
      // Gravel shows through stony ground and spill; sand and traffic bury it.
      const gravel = Math.max(
        0,
        Math.min(
          1,
          0.5 + stony * 0.45 + windrow * 0.35 - wornSand * 0.4 - road * 0.25 - drift * 0.5,
        ),
      );
      const i = ((row - start) * size + col) * 4;
      pixels[i] = r + shade;
      pixels[i + 1] = g + shade;
      pixels[i + 2] = b + shade;
      pixels[i + 3] = 128 + gravel * 127;
    }
  }
  return pixels;
}
