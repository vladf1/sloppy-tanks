import * as THREE from "three/webgpu";
import { float, mix, positionWorld, texture as sampleTexture, uv, vec2, vec4 } from "three/tsl";
import { Random } from "./math";
import { quarryGrit } from "./quarry-grit";
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

let soil: THREE.Texture | undefined;
/** The baked work-yard soil, created and baked once for every surface that must
 * match the ground: the floor itself, spoil heaps, rock feet and drift. */
function quarrySoilTexture(): THREE.Texture {
  if (!soil) {
    if (typeof document === "undefined") {
      // Headless geometry tests build rock materials with no DOM and no bake.
      soil = new THREE.Texture();
    } else {
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = QUARRY_SOIL_SIZE;
      soil = new THREE.CanvasTexture(canvas);
      bakeSoil(canvas, sandAccum(), soil);
    }
    soil.colorSpace = THREE.SRGBColorSpace;
    // WebGPU clamps sampler anisotropy to what the adapter supports.
    soil.anisotropy = 8;
  }
  return soil;
}

/** The baked soil lying under a world position, as the terrain renders it. */
export function quarrySoilAt(position = positionWorld) {
  return sampleTexture(
    quarrySoilTexture(),
    vec2(position.x.div(EXTENT).add(0.5), float(0.5).sub(position.z.div(EXTENT))),
  );
}

/** Baked work-yard soil plus world-space grit. The bake's alpha says how stony
 * each spot is: sand sheets stay smooth, gravel and spill get coarse relief.
 * Meshes using it carry UVs that map world x/z onto the bake. */
function soilMaterial(): THREE.MeshStandardNodeMaterial {
  const material = new THREE.MeshStandardNodeMaterial({ roughness: 1 });
  const baked = sampleTexture(quarrySoilTexture(), uv());
  const stony = baked.a.sub(0.5).mul(2);
  const grit = quarryGrit(mix(0.55, 1.5, stony), mix(0.35, 1.6, stony));
  material.colorNode = vec4(baked.rgb.mul(grit.color), 1);
  material.normalNode = grit.normal;
  material.vertexColors = true;
  return material;
}

/** Soil meshes all carry vertex colors, so plain soil shares its shader with the
 * tinted spoil and ramp; white leaves the baked soil unchanged. */
export function plainSoilColors(geometry: THREE.BufferGeometry): THREE.BufferGeometry {
  const colors = new Float32Array(geometry.getAttribute("position").count * 3).fill(1);
  geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));
  return geometry;
}

/** A baked, metre-scaled work yard: pale sand sheets, compacted haul routes,
 * exposed stony soil, wheel ruts and aggregate. Generated once for the retained
 * scenery, never during round reset or rendering. */
export function quarryTerrain() {
  const extent = EXTENT;
  const material = soilMaterial();
  const geometry = new THREE.PlaneGeometry(extent, extent, 140, 140).rotateX(-Math.PI / 2);
  const positions = geometry.getAttribute("position");
  for (let i = 0; i < positions.count; i++) {
    const outside = Math.max(Math.abs(positions.getX(i)), Math.abs(positions.getZ(i))) - 60;
    positions.setY(i, outside > 0 ? -Math.min(1.8, outside * 0.3) : 0);
  }
  geometry.computeVertexNormals();
  const floor = new THREE.Mesh(plainSoilColors(geometry), material);
  floor.name = "quarry-compacted-haul-roads";
  floor.position.y = 0.008;
  floor.receiveShadow = true;
  return floor;
}
