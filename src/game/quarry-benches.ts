import * as THREE from "three";
import { batch } from "./batching";
import { Random } from "./math";
import { sandstoneMaterial, sandstoneRock } from "./quarry-surfaces";

/** Continuous blasted quarry face with a walkable-looking shelf between each cut.
 * Scenery only: the innermost face is beyond the arena and machinery apron. */
export function quarryBench(
  length: number,
  height: number,
  depth: number,
  seed: number,
): THREE.Mesh {
  const rng = new Random(seed);
  const positions: number[] = [];
  const colors: number[] = [];
  const uvs: number[] = [];
  const indices: number[] = [];
  const segments = Math.ceil(length / 2.8);
  // Each wall reads differently: overall lean plus long swells along its length.
  const lean = 1 + 0.1 * Math.sin(seed * 2.3) + 0.06 * Math.sin(seed * 5.1);
  const rows = [0, 0.08, 0.28, 0.32, 0.58, 0.62, 0.88, 1, 1];
  for (let i = 0; i <= segments; i++) {
    const x = (i / segments - 0.5) * length;
    const fracture = Math.sin(x * 0.13 + seed) * 0.5 + rng.range(-0.18, 0.18);
    const crown =
      0.74 +
      0.2 * Math.sin(x * 0.05 + seed * 1.7) +
      0.1 * Math.sin(x * 0.13 + seed) +
      rng.range(-0.08, 0.08);
    const shelfInset = -0.18 + 0.1 * Math.sin(x * 0.07 + seed * 0.9);
    for (let ring = 0; ring < rows.length; ring++) {
      const t = rows[ring];
      const shelf = ring === 3 || ring === 5;
      const y = t * height * crown * lean;
      const z = ring === 8 ? depth : t * 2.8 + fracture + (shelf ? shelfInset : 0);
      positions.push(x, y, z);
      uvs.push(x / 5, ring === 8 ? z / 5 : y / 5);
      // Warm sedimentary banding runs with height; shelves sit in shade.
      const band = 1 + 0.05 * Math.sin(y * 1.15 + seed * 2.0);
      const shade = (ring === 2 || ring === 4 ? 0.74 : 0.95) + rng.range(-0.04, 0.06);
      colors.push(shade * band, shade * (1 + (band - 1) * 0.6), shade * (1 + (band - 1) * 0.2));
      if (i < segments && ring < rows.length - 1) {
        const a = i * rows.length + ring;
        const b = a + rows.length;
        indices.push(a, a + 1, b, b, a + 1, b + 1);
      }
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
  geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
  geometry.setIndex(indices);
  // Broken flat normals make the vertical blast fractures read in grazing light.
  const fractured = geometry.toNonIndexed();
  geometry.dispose();
  fractured.computeVertexNormals();
  const mesh = new THREE.Mesh(fractured, sandstoneMaterial());
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}

export interface ScreeSpot {
  x: number;
  z: number;
  rotY: number;
  length: number;
  height: number;
  depth: number;
  seed: number;
}

/** Localized collapses interrupting the first terrace. Every footprint stays on
 * the machinery apron, clear of the playable boundary and the haul loop. */
export function quarryScreeSpots(): ScreeSpot[] {
  return [
    { x: -32, z: 70, rotY: 0, length: 26, height: 3.6, depth: 12, seed: 11 },
    { x: 16, z: 70, rotY: 0, length: 20, height: 3.1, depth: 12, seed: 23 },
    { x: -6, z: -70, rotY: Math.PI, length: 24, height: 3.4, depth: 11, seed: 37 },
    { x: 36, z: -70, rotY: Math.PI, length: 18, height: 3.0, depth: 11, seed: 49 },
    { x: -70, z: 6, rotY: -Math.PI / 2, length: 22, height: 3.3, depth: 14, seed: 61 },
    { x: 70, z: -12, rotY: Math.PI / 2, length: 20, height: 3.2, depth: 12, seed: 73 },
  ];
}

export interface ButteSpot {
  x: number;
  z: number;
  baseY: number;
  scale: number;
  rotY: number;
}

/** A lone layered sentinel on the north apron, clear of the boundary, the
 * conveyor, the excavator swing and both northern scree collapses. Centered so
 * the whole northern play band sees it, not just the north-west corner. */
export function quarryButteSpot(): ButteSpot {
  return { x: 16, z: -68, baseY: -1.8, scale: 1, rotY: 0.15 };
}

// w, h, d, dx, dy, dz per stacked slab.
const BUTTE_SLABS: [number, number, number, number, number, number][] = [
  [15, 3.2, 11, 0, 1.6, 0],
  [12.5, 2.8, 9.5, 0.9, 4.6, -0.5],
  [10, 2.6, 8, -0.7, 7.3, 0.6],
  [7.6, 2.4, 6.2, 0.5, 9.9, -0.4],
  [5, 2.2, 4.4, -0.4, 12.2, 0.3],
  [3.4, 1.8, 3, 0.3, 14, -0.2],
];

/** World-space footprint corners of the stacked slabs, for placement checks. */
export function quarryButteFootprint(spot: ButteSpot = quarryButteSpot()): [number, number][] {
  const corners: [number, number][] = [];
  BUTTE_SLABS.forEach(([w, , d, dx, , dz], i) => {
    const a = spot.rotY + i * 0.22;
    const c = Math.cos(a);
    const s = Math.sin(a);
    for (const ex of [-1, 1]) {
      for (const ez of [-1, 1]) {
        const lx = (ex * (w / 2) + dx) * spot.scale;
        const lz = (ez * (d / 2) + dz) * spot.scale;
        corners.push([spot.x + lx * c + lz * s, spot.z - lx * s + lz * c]);
      }
    }
  });
  return corners;
}

/** Stacked offset slabs with an eroded cap: one recognizable formation. The
 * slabs merge into a single batch, so the landmark costs one draw call. */
export function quarryButte(scale = 1, rotY = 0): THREE.Group {
  const group = new THREE.Group();
  group.name = "quarry-sentinel-butte";
  BUTTE_SLABS.forEach(([w, h, d, dx, dy, dz], i) => {
    const rock = sandstoneRock(w * scale, h * scale, d * scale, 20 + i);
    rock.position.set(dx * scale, dy * scale, dz * scale);
    rock.rotation.y = rotY + i * 0.22;
    group.add(rock);
  });
  batch(group);
  return group;
}
