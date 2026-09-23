import * as THREE from "three";
import { batch } from "./batching";
import { Random } from "./math";
import { sandstoneMaterial, sandstoneRock } from "./quarry-surfaces";

// Fractions of the face height, toe to crest; (0.3, 0.34) and (0.62, 0.66) are
// the narrow ledges left between blast lifts.
const FACE_ROWS = [0, 0.07, 0.22, 0.3, 0.34, 0.5, 0.62, 0.66, 0.8, 0.93, 1];
const LEDGES = new Set([3, 5, 6]);

/** Continuous blasted quarry face with a safety berm along its crest and a
 * walkable-looking shelf behind. Blocky relief, jittered lifts and flat-shaded
 * facets keep the long walls from reading as extruded bands. Scenery only: the
 * innermost face is beyond the arena and machinery apron. */
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
  const segments = Math.ceil(length / 2.2);
  // Each wall reads differently: overall lean plus long swells along its length.
  const lean = 1 + 0.1 * Math.sin(seed * 2.3) + 0.06 * Math.sin(seed * 5.1);
  const rows = FACE_ROWS.length + 3;
  for (let i = 0; i <= segments; i++) {
    const edge = i === 0 || i === segments;
    const x = (i / segments - 0.5) * length + (edge ? 0 : rng.range(-0.55, 0.55));
    const fracture = Math.sin(x * 0.13 + seed) * 0.5 + rng.range(-0.3, 0.3);
    const crown =
      0.74 +
      0.2 * Math.sin(x * 0.05 + seed * 1.7) +
      0.1 * Math.sin(x * 0.13 + seed) +
      rng.range(-0.08, 0.08);
    const top = height * crown * lean;
    const shelfInset = -0.18 + 0.1 * Math.sin(x * 0.07 + seed * 0.9);
    // Blast blocks stand proud or break back as coherent columns.
    const block = Math.sin(x * 0.83 + seed * 3.1) * 0.35 + rng.range(-0.2, 0.2);
    const crest = 2.8 + fracture;
    for (let ring = 0; ring < rows; ring++) {
      let y: number;
      let z: number;
      if (ring < FACE_ROWS.length) {
        const t = FACE_ROWS[ring];
        const inner = ring > 0 && ring < FACE_ROWS.length - 1;
        y = t * top + (inner ? rng.range(-0.035, 0.035) * top : 0);
        z =
          t * 2.8 +
          fracture +
          (LEDGES.has(ring) ? shelfInset : 0) +
          (inner ? block * Math.sin(t * Math.PI) + rng.range(-0.22, 0.22) : 0);
      } else if (ring === FACE_ROWS.length) {
        // Windrowed safety berm pushed up just behind the crest.
        y = top + 0.5 + rng.range(-0.12, 0.18);
        z = crest + 1.4 + rng.range(-0.25, 0.25);
      } else if (ring === FACE_ROWS.length + 1) {
        y = top + 0.04;
        z = crest + 2.8;
      } else {
        y = top;
        z = depth;
      }
      positions.push(x, y, z);
      const flat = ring >= FACE_ROWS.length - 1;
      uvs.push(x / 5, flat ? z / 5 : y / 5);
      // Warm sedimentary banding runs with height; ledge undersides sit in shade.
      const band = 1 + 0.05 * Math.sin(y * 1.15 + seed * 2.0);
      const shade = (ring === 2 || ring === 4 || ring === 7 ? 0.74 : 0.95) + rng.range(-0.05, 0.07);
      colors.push(shade * band, shade * (1 + (band - 1) * 0.6), shade * (1 + (band - 1) * 0.2));
      if (i < segments && ring < rows - 1) {
        const a = i * rows + ring;
        const b = a + rows;
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

/** Talus heaped along a wall toe, in the wall's local frame: x along the face,
 * z toward the face (the rear buries itself 1.2 m inside), y up from its base.
 * Reach and height swell and pinch along the wall, near the angle of repose. */
export function quarryTalusPoint(x: number, t: number, seed: number): THREE.Vector3 {
  const swell = 0.5 + 0.3 * Math.sin(x * 0.071 + seed) + 0.2 * Math.sin(x * 0.23 + seed * 1.9);
  const reach = 1.6 + 3.2 * swell;
  const z = -reach + t * (reach + 1.2);
  // Concave toe steepening toward the face, with lumpy slump.
  const lump = Math.sin(x * 1.3 + t * 5.1 + seed) * Math.sin(x * 0.47 - t * 3.3) * 0.16;
  const y = reach * 0.68 * Math.pow(t, 1.45) + lump * Math.sin(t * Math.PI) - 0.2 * (1 - t);
  return new THREE.Vector3(x, y, z);
}

/** Continuous talus strip spanning x0..x1, in the wall's local frame. */
export function quarryTalusGeometry(x0: number, x1: number, seed: number): THREE.BufferGeometry {
  const across = Math.ceil((x1 - x0) / 1.1);
  const rows = 6;
  const positions: number[] = [];
  const indices: number[] = [];
  for (let row = 0; row <= rows; row++) {
    for (let col = 0; col <= across; col++) {
      const x = x0 + ((x1 - x0) * col) / across;
      positions.push(...quarryTalusPoint(x, row / rows, seed).toArray());
      if (row < rows && col < across) {
        const a = row * (across + 1) + col;
        const b = a + across + 1;
        indices.push(a, b, a + 1, b, b + 1, a + 1);
      }
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}

export interface TalusStrip {
  /** Toe of the lowest cut; the strip shares that wall's orientation. */
  x: number;
  z: number;
  rotY: number;
  /** Span along the wall, in its local x. */
  x0: number;
  x1: number;
  seed: number;
}

/** Talus along the four lowest cuts, matching their toes in quarry-scenery. The
 * east strip stops short of the haul ramp; corners tuck into the flank walls. */
export function quarryTalusStrips(): TalusStrip[] {
  return [
    { x: 0, z: -77, rotY: Math.PI, x0: -80, x1: 80, seed: 5 },
    { x: 0, z: 78, rotY: 0, x0: -80, x1: 80, seed: 9 },
    { x: -80, z: 0, rotY: -Math.PI / 2, x0: -77, x1: 78, seed: 13 },
    { x: 78, z: 0, rotY: Math.PI / 2, x0: -22, x1: 77, seed: 17 },
  ];
}

export interface StockpileSpot {
  x: number;
  z: number;
  radius: number;
  height: number;
  seed: number;
}

/** Crushed stone heaped under the screening conveyor's head drum. */
export function quarryStockpileSpot(): StockpileSpot {
  return { x: -41, z: -71.5, radius: 6.4, height: 4.4, seed: 3 };
}

/** Radius of the pile's toe at a bearing; the long axis follows the conveyor. */
export function quarryStockpileReach(spot: StockpileSpot, angle: number): number {
  const { radius, seed } = spot;
  return (
    radius *
    (1 +
      0.14 * Math.cos(angle) ** 2 +
      0.07 * Math.sin(angle * 3 + seed) +
      0.04 * Math.sin(angle * 7))
  );
}

/** Conical stockpile at the angle of repose, slumped and lumpy, with a base
 * buried below the apron. Local frame, centred on its apex axis. */
export function quarryStockpileGeometry(spot: StockpileSpot): THREE.BufferGeometry {
  const rings = 9;
  const segments = 36;
  const positions: number[] = [0, spot.height, 0];
  const indices: number[] = [];
  for (let ring = 1; ring <= rings; ring++) {
    const t = ring / rings;
    for (let side = 0; side < segments; side++) {
      const angle = (side / segments) * Math.PI * 2;
      const reach = quarryStockpileReach(spot, angle) * t;
      // A rounded crest where the stream lands, then a straight repose slope.
      const lump = Math.sin(angle * 11 + t * 9 + spot.seed) * 0.09 * t;
      const y = ring === rings ? -0.25 : spot.height * (1 - t ** 1.08) + lump;
      positions.push(Math.cos(angle) * reach, y, Math.sin(angle) * reach);
      const a = 1 + (ring - 1) * segments + side;
      const b = 1 + (ring - 1) * segments + ((side + 1) % segments);
      if (ring === 1) {
        indices.push(0, b, a);
      } else {
        indices.push(a - segments, b - segments, a, b - segments, b, a);
      }
    }
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
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
