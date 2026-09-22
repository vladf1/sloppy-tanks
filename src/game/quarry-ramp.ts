import * as THREE from "three";
import { Random } from "./math";
import { QUARRY_TERRAIN_EXTENT } from "./quarry-terrain";

/** Haul ramp climbing the east cut from the pit floor to the first shelf. The
 * deck rises northward against the wall; a rock berm guards the open edge and
 * fill slopes bury into the apron. Scenery only: no collider or navigation. */
export const QUARRY_RAMP = {
  /** Apron floor level beyond the playable boundary. */
  floor: 0.008 - 1.8,
  /** Landing height, a few centimetres proud of the lowest stretch of shelf. */
  crest: 1.9,
  /** North edge of the landing, where the berm turns along the end. */
  zCrest: 33,
  /** Where the landing ends and the grade begins. */
  zLanding: 38,
  /** Where the deck meets the apron floor. */
  zFoot: 66,
  /** Berm centerline along the open (west) edge of the deck. */
  xBerm: 72.2,
  /** Past the shelf lip the landing tucks under; the wall hides the grade. */
  xInner: 80.9,
  /** Grid bounds, including the buried toes of every fill slope. */
  x0: 66,
  x1: 84.5,
  z0: 26,
  z1: 68,
} as const;

const BERM_HALF = 0.6;
const BERM_HEIGHT = 0.55;
/** Rise per metre of the loose fill flanks, near the angle of repose. */
const FILL = 0.8;
const RUTS = [74.3, 76.9];

function deckHeight(z: number): number {
  const { floor, crest, zLanding, zFoot } = QUARRY_RAMP;
  if (z >= zFoot) {
    return floor - (z - zFoot) * 0.3;
  }
  const t = THREE.MathUtils.clamp((z - zLanding) / (zFoot - zLanding), 0, 1);
  return THREE.MathUtils.lerp(crest, floor, THREE.MathUtils.smoothstep(t, 0, 1));
}

/** Surface elevation, including berm and fill slopes; buried points clamp below the apron. */
export function quarryRampHeight(x: number, z: number): number {
  const { floor, zCrest, zFoot, xBerm, xInner } = QUARRY_RAMP;
  const u = x - xBerm;
  const v = z - (zCrest - BERM_HALF);
  const outside = Math.hypot(Math.max(0, -BERM_HALF - u), Math.max(0, -BERM_HALF - v));
  const bump = (s: number) => Math.max(0, 1 - (s / BERM_HALF) ** 2);
  const fade = 1 - THREE.MathUtils.smoothstep(z, zFoot - 8, zFoot);
  const berm = BERM_HEIGHT * fade * Math.max(bump(u), bump(v));
  const relief = outside > 0 ? Math.sin(x * 1.7 + z * 0.9) * Math.sin(z * 2.3 - x) * 0.09 : 0;
  const y =
    deckHeight(Math.max(z, zCrest)) +
    berm -
    outside * FILL +
    relief -
    Math.max(0, x - xInner) * 2.5;
  return Math.max(y, floor - 0.25);
}

function rampColor(x: number, z: number, y: number): [number, number, number] {
  const { floor, zCrest, xBerm } = QUARRY_RAMP;
  const deck = x > xBerm + BERM_HALF && z > zCrest;
  if (!deck) {
    // Loose sandstone spoil: warmer than the apron, fading into it at the toe.
    const lift = THREE.MathUtils.smoothstep(y - floor, 0, 1.4);
    return [1 - 0.02 * lift, 1 - 0.1 * lift, 1 - 0.2 * lift];
  }
  // Compacted haul deck with two darker wheel ruts; it lightens into the apron
  // as it reaches floor level so the shared texture meets without a seam.
  const rut = Math.max(...RUTS.map((r) => Math.exp(-(((x - r) / 0.45) ** 2))));
  const compacted = 0.74 - rut * 0.14;
  const k = THREE.MathUtils.smoothstep(y - floor, 0, 0.6);
  return [
    THREE.MathUtils.lerp(1, compacted, k),
    THREE.MathUtils.lerp(1, compacted * 0.97, k),
    THREE.MathUtils.lerp(1, compacted * 0.93, k),
  ];
}

/** Heightfield deck sharing the apron texture at world coordinates. */
export function quarryRampGeometry(): THREE.BufferGeometry {
  const { x0, x1, z0, z1 } = QUARRY_RAMP;
  const cols = Math.ceil((x1 - x0) / 0.4);
  const rows = Math.ceil((z1 - z0) / 0.7);
  const positions: number[] = [];
  const colors: number[] = [];
  const uvs: number[] = [];
  const indices: number[] = [];
  for (let row = 0; row <= rows; row++) {
    const z = z0 + ((z1 - z0) * row) / rows;
    for (let col = 0; col <= cols; col++) {
      const x = x0 + ((x1 - x0) * col) / cols;
      const y = quarryRampHeight(x, z);
      positions.push(x, y, z);
      colors.push(...rampColor(x, z, y));
      uvs.push(x / QUARRY_TERRAIN_EXTENT + 0.5, 0.5 - z / QUARRY_TERRAIN_EXTENT);
      if (row < rows && col < cols) {
        const a = row * (cols + 1) + col;
        const b = a + cols + 1;
        indices.push(a, b, a + 1, b, b + 1, a + 1);
      }
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  geometry.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}

/** Windrow boulders spaced along the berm crest, in world coordinates. */
export function quarryRampBoulders(): { x: number; z: number; size: number; rotY: number }[] {
  const { zCrest, zFoot, xBerm } = QUARRY_RAMP;
  const rng = new Random(4417);
  const boulders = [];
  for (let z = zCrest + 1.5; z < zFoot - 7; z += rng.range(3.2, 5.4)) {
    boulders.push({
      x: xBerm + rng.range(-0.25, 0.25),
      z,
      size: rng.range(0.7, 1.25),
      rotY: rng.range(-Math.PI, Math.PI),
    });
  }
  for (let x = xBerm + 2.5; x < QUARRY_RAMP.xInner - 1; x += rng.range(2.6, 3.4)) {
    boulders.push({ x, z: zCrest - BERM_HALF, size: rng.range(0.7, 1.1), rotY: rng.range(-1, 1) });
  }
  return boulders;
}

/** Loose spoil chips strewn down the open fill flank, denser toward the toe. */
export function quarryRampSpoil(): { x: number; z: number; size: number; rotY: number }[] {
  const { zCrest, zFoot, xBerm } = QUARRY_RAMP;
  const rng = new Random(9023);
  const chips = [];
  for (let i = 0; i < 70; i++) {
    const z = rng.range(zCrest - 3, zFoot - 6);
    const height = deckHeight(Math.max(z, zCrest)) - QUARRY_RAMP.floor;
    const reach = (height + BERM_HEIGHT) / FILL;
    const x = xBerm - BERM_HALF - reach * Math.sqrt(rng.range(0.05, 1.1));
    chips.push({ x, z, size: rng.range(0.25, 0.7), rotY: rng.range(-Math.PI, Math.PI) });
  }
  return chips;
}
