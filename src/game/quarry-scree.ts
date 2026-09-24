import * as THREE from "three";
import { mergeGeometries } from "three/addons/utils/BufferGeometryUtils.js";
import { Random } from "./math";
import type { ScreeSpot } from "./quarry-benches";
import { roughenStone, sandstoneMaterial } from "./quarry-surfaces";

const stoneCorner = new THREE.Vector3();
import { QUARRY_TERRAIN_EXTENT, plainSoilColors } from "./quarry-terrain";

/** A fan of sediment, with scalloped toes and sides buried below the apron.
 * The high back extends into the quarry cut so it cannot expose a thin lip. */
function screePoint(spot: ScreeSpot, u: number, t: number): THREE.Vector3 {
  const { length, height, depth, seed } = spot;
  const side = Math.max(0, 1 - u * u);
  const toe = 0.4 + 1.3 * (0.5 + 0.5 * Math.sin(u * 11 + seed)) + 1.5 * u * u;
  const crest = 0.87 + 0.13 * Math.sin(u * 7 + seed);
  const relief = Math.sin(u * 23 + t * 17 + seed) * 0.14 * Math.sin(t * Math.PI);
  return new THREE.Vector3(
    (u * length * (1 - t * 0.18)) / 2,
    side * (Math.pow(t, 1.2) * height * crest + relief) - 0.16,
    toe + t * (depth - toe),
  );
}

/** Closed volume, including a buried underside; useful independently of textures. */
export function quarryScreeGeometry(spot: ScreeSpot): THREE.BufferGeometry {
  const across = Math.ceil(spot.length / 0.8);
  const rows = 12;
  const positions: number[] = [];
  const indices: number[] = [];
  for (let row = 0; row <= rows; row++) {
    for (let col = 0; col <= across; col++) {
      positions.push(...screePoint(spot, (col / across) * 2 - 1, row / rows).toArray());
      if (row < rows && col < across) {
        const a = row * (across + 1) + col;
        const b = a + across + 1;
        indices.push(a, b, a + 1, b, b + 1, a + 1);
      }
    }
  }
  const rim = [
    ...Array.from({ length: across }, (_, i) => i),
    ...Array.from({ length: rows }, (_, i) => i * (across + 1) + across),
    ...Array.from({ length: across }, (_, i) => rows * (across + 1) + across - i),
    ...Array.from({ length: rows }, (_, i) => (rows - i) * (across + 1)),
  ];
  const bottom = positions.length / 3;
  for (const index of rim) {
    positions.push(positions[index * 3], -0.5, positions[index * 3 + 2]);
  }
  const center = positions.length / 3;
  positions.push(0, -0.5, spot.depth / 2);
  for (let i = 0; i < rim.length; i++) {
    const next = (i + 1) % rim.length;
    indices.push(rim[i], rim[next], bottom + next, rim[i], bottom + next, bottom + i);
    indices.push(center, bottom + i, bottom + next);
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setIndex(indices);
  geometry.computeVertexNormals();
  return geometry;
}

/** Angular fragments, sparse at the toe and coarser toward the cut. All randomness
 * is local to scenery; one merged mesh keeps hundreds of stones out of the scene graph. */
export function quarryScreeRubble(spot: ScreeSpot): THREE.BufferGeometry {
  const rng = new Random(spot.seed * 131 + 7);
  const pieces: THREE.BufferGeometry[] = [];
  const template = new THREE.IcosahedronGeometry(1, 0);
  const count = Math.ceil(spot.length * spot.depth * 1.1);
  for (let i = 0; i < count; i++) {
    const u = rng.range(-0.97, 0.97);
    const t = rng.range(0.04, 0.98);
    const point = screePoint(spot, u, t);
    const size = rng.range(0.14, 0.48) * (0.65 + t * 1.1);
    const large = i % 11 === 0 ? 1.8 : 1;
    const geometry = template.clone();
    const corners = geometry.getAttribute("position");
    for (let v = 0; v < corners.count; v++) {
      roughenStone(stoneCorner.fromBufferAttribute(corners, v), i + spot.seed * 1000);
      corners.setXYZ(v, stoneCorner.x, stoneCorner.y, stoneCorner.z);
    }
    geometry.scale(
      size * large,
      size * rng.range(0.45, 0.85),
      size * rng.range(0.65, 1.25) * large,
    );
    geometry.rotateX(rng.range(-0.4, 0.4));
    geometry.rotateY(rng.range(-Math.PI, Math.PI));
    geometry.rotateZ(rng.range(-0.3, 0.3));
    geometry.translate(point.x, Math.max(0, point.y) + size * 0.15, point.z);
    // Flat fracture faces and varied dust deposits break up the bedrock grain.
    geometry.computeVertexNormals();
    const colors = new Float32Array(geometry.getAttribute("position").count * 3);
    const shade = rng.range(0.74, 1.08);
    for (let c = 0; c < colors.length; c += 3) {
      colors[c] = shade;
      colors[c + 1] = shade;
      colors[c + 2] = shade * 0.97;
    }
    geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));
    pieces.push(geometry);
  }
  const rubble = mergeGeometries(pieces);
  template.dispose();
  for (const piece of pieces) {
    piece.dispose();
  }
  return rubble;
}

export function quarryScree(spot: ScreeSpot, soil: THREE.Material): THREE.Mesh[] {
  const mound = quarryScreeGeometry(spot);
  const rubble = quarryScreeRubble(spot);
  const dip = Math.min(1.8, (Math.max(Math.abs(spot.x), Math.abs(spot.z)) - 60) * 0.3);
  for (const geometry of [mound, rubble]) {
    geometry.rotateY(spot.rotY);
    geometry.translate(spot.x, 0.008 - dip, spot.z);
  }
  // Sample the actual ground texture at world coordinates, making the buried
  // perimeter continuous with the apron instead of painting a rectangular patch.
  const positions = mound.getAttribute("position");
  const uvs: number[] = [];
  for (let i = 0; i < positions.count; i++) {
    uvs.push(
      positions.getX(i) / QUARRY_TERRAIN_EXTENT + 0.5,
      0.5 - positions.getZ(i) / QUARRY_TERRAIN_EXTENT,
    );
  }
  mound.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
  return [
    new THREE.Mesh(plainSoilColors(mound), soil),
    new THREE.Mesh(rubble, sandstoneMaterial()),
  ];
}
