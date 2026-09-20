import * as THREE from "three";
import { Random } from "./math";
import { quarryLayout } from "./quarry-layout";

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
  return THREE.MathUtils.lerp(
    THREE.MathUtils.lerp(hash(ix, iz), hash(ix + 1, iz), u),
    THREE.MathUtils.lerp(hash(ix, iz + 1), hash(ix + 1, iz + 1), u),
    v,
  );
}

const EXTENT = 210;
const ACCUM_CELLS = 105;

/** Windblown sand piled against cover: splatted once per layout, sampled per pixel. */
let accumGrid: Float32Array | undefined;
function sandAccum(): Float32Array {
  if (accumGrid) {
    return accumGrid;
  }
  const grid = new Float32Array(ACCUM_CELLS * ACCUM_CELLS);
  const rng = new Random(514);
  const cell = EXTENT / (ACCUM_CELLS - 1);
  const splat = (x: number, z: number, radius: number, strength: number) => {
    const cx = (x + EXTENT / 2) / cell;
    const cz = (z + EXTENT / 2) / cell;
    const r = radius / cell;
    const i0 = Math.max(0, Math.floor(cx - r));
    const i1 = Math.min(ACCUM_CELLS - 1, Math.ceil(cx + r));
    const j0 = Math.max(0, Math.floor(cz - r));
    const j1 = Math.min(ACCUM_CELLS - 1, Math.ceil(cz + r));
    for (let j = j0; j <= j1; j++) {
      for (let i = i0; i <= i1; i++) {
        const d = Math.hypot(i - cx, j - cz) / r;
        if (d < 1) {
          const fall = (1 - d * d) * (1 - d * d);
          grid[j * ACCUM_CELLS + i] += strength * fall;
        }
      }
    }
  };
  for (const cover of quarryLayout()) {
    if (cover.kind === "boundary") {
      continue;
    }
    // Overlapping blobs biased downwind (+x) read as drift, not stamped circles.
    const base = Math.max(cover.w, cover.d) * 0.5 + 1.4;
    for (let k = 0; k < 3; k++) {
      splat(
        cover.x + rng.range(-1.2, 2.6),
        cover.z + rng.range(-2.2, 2.2),
        base * rng.range(0.7, 1.15),
        0.3,
      );
    }
  }
  accumGrid = grid;
  return grid;
}

function sampleAccum(grid: Float32Array, x: number, z: number): number {
  const cell = EXTENT / (ACCUM_CELLS - 1);
  const fx = THREE.MathUtils.clamp((x + EXTENT / 2) / cell, 0, ACCUM_CELLS - 1.001);
  const fz = THREE.MathUtils.clamp((z + EXTENT / 2) / cell, 0, ACCUM_CELLS - 1.001);
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

/** A baked, metre-scaled work yard: pale sand sheets, dark compacted haul routes,
 * exposed rocky soil, wheel ruts and aggregate. Generated once for the retained
 * scenery, never during round reset or rendering. */
export function quarryTerrain(renderer: THREE.WebGLRenderer): THREE.Mesh {
  const size = 2048;
  const extent = EXTENT;
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext("2d")!;
  const pixels = ctx.createImageData(size, size);
  const accum = sandAccum();
  const rng = new Random(7391);
  const smooth = THREE.MathUtils.smoothstep;
  for (let row = 0; row < size; row++) {
    const z = (row / (size - 1) - 0.5) * extent;
    for (let col = 0; col < size; col++) {
      const x = (col / (size - 1) - 0.5) * extent;
      const macro = noise(x * 0.065, z * 0.065);
      const grit = noise(x * 0.75, z * 0.75);
      // Broad irregular sheets: pale windblown sand where the macro field runs
      // high, exposed rocky soil where it runs low. A sine warp breaks any hint
      // of cellular repetition without adding another noise octave to the bake.
      const warp = Math.sin(x * 0.021 + 1.7) * Math.sin(z * 0.023 - 0.6);
      const sand = smooth(macro + warp * 0.18, 0.52, 0.78);
      const rocky = 1 - smooth(macro - warp * 0.15, 0.22, 0.48);
      // Rounded outer haul loop and a gently wandering east/west crossing.
      const qx = Math.abs(x) - 39;
      const qz = Math.abs(z) - 39;
      const loop = Math.abs(
        Math.hypot(Math.max(qx, 0), Math.max(qz, 0)) + Math.min(Math.max(qx, qz), 0) - 12,
      );
      const crossing = Math.abs(z - Math.sin(x * 0.055) * 1.6);
      const distance = Math.min(loop, crossing);
      const edge = (noise(x * 0.42, z * 0.42) - 0.5) * 1.6;
      const road = 1 - smooth(distance + edge, 3.2, 7.8);
      const rut = Math.exp(-Math.pow((distance - 2.25) / 0.38, 2)) * road;
      const fine = rng.range(-5, 5);
      const aggregate = rng.next() > 0.975 ? rng.range(-22, 17) : 0;
      // sRGB bytes: mid soil -> pale sand -> rocky soil -> dark compacted road.
      let r = 181;
      let g = 157;
      let b = 122;
      r += (221 - r) * sand;
      g += (200 - g) * sand;
      b += (162 - b) * sand;
      const offRoad = 1 - road;
      r += (160 - r) * rocky * offRoad;
      g += (130 - g) * rocky * offRoad;
      b += (99 - b) * rocky * offRoad;
      r += (146 - r) * road * 0.9;
      g += (122 - g) * road * 0.9;
      b += (94 - b) * road * 0.9;
      const shade = (macro - 0.5) * 8 + (grit - 0.5) * 15 + fine + aggregate - rut * 10;
      // Drifted sand against cover bases and along the quiet outer shoulders.
      const shoulder = smooth(Math.max(Math.abs(x), Math.abs(z)), 48, 58) * offRoad;
      const drift = Math.min(0.55, sampleAccum(accum, x, z) * 0.55 + shoulder * 0.4);
      r += (226 - r) * drift;
      g += (209 - g) * drift;
      b += (176 - b) * drift;
      const i = (row * size + col) * 4;
      pixels.data[i] = r + shade;
      pixels.data[i + 1] = g + shade;
      pixels.data[i + 2] = b + shade;
      pixels.data[i + 3] = 255;
    }
  }
  ctx.putImageData(pixels, 0, 0);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = Math.min(8, renderer.capabilities.getMaxAnisotropy());
  const material = new THREE.MeshStandardMaterial({
    map: texture,
    bumpMap: texture,
    bumpScale: 0.065,
    roughness: 1,
  });
  const geometry = new THREE.PlaneGeometry(extent, extent, 140, 140).rotateX(-Math.PI / 2);
  const positions = geometry.getAttribute("position");
  for (let i = 0; i < positions.count; i++) {
    const outside = Math.max(Math.abs(positions.getX(i)), Math.abs(positions.getZ(i))) - 60;
    positions.setY(i, outside > 0 ? -Math.min(1.8, outside * 0.3) : 0);
  }
  geometry.computeVertexNormals();
  const floor = new THREE.Mesh(geometry, material);
  floor.name = "quarry-compacted-haul-roads";
  floor.position.y = 0.008;
  floor.receiveShadow = true;
  return floor;
}
