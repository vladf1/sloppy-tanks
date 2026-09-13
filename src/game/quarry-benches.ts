import * as THREE from "three";
import { Random } from "./math";
import { sandstoneMaterial } from "./quarry-surfaces";

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
  const rows = [0, 0.08, 0.28, 0.32, 0.58, 0.62, 0.88, 1, 1];
  for (let i = 0; i <= segments; i++) {
    const x = (i / segments - 0.5) * length;
    const fracture = Math.sin(x * 0.13 + seed) * 0.5 + rng.range(-0.18, 0.18);
    const crown = 0.86 + 0.1 * Math.sin(x * 0.13 + seed) + rng.range(-0.055, 0.055);
    for (let ring = 0; ring < rows.length; ring++) {
      const t = rows[ring];
      const shelf = ring === 3 || ring === 5;
      const y = t * height * crown;
      const z = ring === 8 ? depth : t * 2.8 + fracture + (shelf ? -0.18 : 0);
      positions.push(x, y, z);
      uvs.push(x / 5, ring === 8 ? z / 5 : y / 5);
      const shade = (ring === 2 || ring === 4 ? 0.65 : 0.87) + rng.range(-0.05, 0.09);
      colors.push(shade, shade * 0.98, shade * 0.94);
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
