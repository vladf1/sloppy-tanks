import * as THREE from "three/webgpu";
import { Random } from "./math";
import { quarryLayout } from "./quarry-layout";
import {
  ACCUM_CELLS,
  bakeQuarrySoil,
  QUARRY_SOIL_SIZE,
  QUARRY_TERRAIN_EXTENT,
} from "./quarry-soil";

export { QUARRY_TERRAIN_EXTENT };
const EXTENT = QUARRY_TERRAIN_EXTENT;

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

const BAKE_KEY = "sloppy:quarry-soil";

/** Bake the 2048² soil off the main thread, split across a few cores. The
 * loading manager tracks the bake like an image, so prepare() waits for it. */
function bakeSoil(canvas: HTMLCanvasElement, accum: Float32Array, texture: THREE.Texture): void {
  const size = QUARRY_SOIL_SIZE;
  const manager = THREE.DefaultLoadingManager;
  manager.itemStart(BAKE_KEY);
  const finish = (bands: { start: number; pixels: Uint8ClampedArray<ArrayBuffer> }[]) => {
    const ctx = canvas.getContext("2d")!;
    for (const { start, pixels } of bands) {
      ctx.putImageData(new ImageData(pixels, size, pixels.length / (size * 4)), 0, start);
    }
    texture.needsUpdate = true;
  };
  const bakeHere = () => finish([{ start: 0, pixels: bakeQuarrySoil(accum) }]);
  const workers = Math.max(1, Math.min(4, (navigator.hardwareConcurrency || 2) - 1));
  const bands = Array.from({ length: workers }, (_, i) => ({
    start: Math.round((i * size) / workers),
    end: Math.round(((i + 1) * size) / workers),
  }));
  Promise.all(
    bands.map(
      ({ start, end }) =>
        new Promise<{ start: number; pixels: Uint8ClampedArray<ArrayBuffer> }>(
          (resolve, reject) => {
            const worker = new Worker(new URL("./quarry-soil-worker.ts", import.meta.url), {
              type: "module",
            });
            worker.onmessage = (event: MessageEvent<Uint8ClampedArray<ArrayBuffer>>) => {
              worker.terminate();
              resolve({ start, pixels: event.data });
            };
            worker.onerror = (event) => {
              worker.terminate();
              reject(event.error instanceof Error ? event.error : new Error(event.message));
            };
            worker.postMessage({ accum, start, end });
          },
        ),
    ),
  )
    .then(finish, (error: unknown) => {
      console.warn("Soil bake worker failed; baking on the main thread.", error);
      bakeHere();
    })
    .finally(() => manager.itemEnd(BAKE_KEY));
}

/** A baked, metre-scaled work yard: pale sand sheets, dark compacted haul routes,
 * exposed rocky soil, wheel ruts and aggregate. Generated once for the retained
 * scenery, never during round reset or rendering. */
export function quarryTerrain(renderer: THREE.WebGPURenderer) {
  const size = QUARRY_SOIL_SIZE;
  const extent = EXTENT;
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = Math.min(8, renderer.getMaxAnisotropy());
  bakeSoil(canvas, sandAccum(), texture);
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
