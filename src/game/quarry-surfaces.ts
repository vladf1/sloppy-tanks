import * as THREE from "three";
import { Random } from "./math";
import { quarryRockShape } from "./quarry-rock-shape";

let stone: THREE.MeshStandardMaterial | undefined;
const geometries = new Map<string, THREE.BufferGeometry>();

export function sandstoneMaterial(): THREE.MeshStandardMaterial {
  if (!stone) {
    const texture = new THREE.TextureLoader().load(
      `${import.meta.env?.BASE_URL ?? "/"}textures/quarry/sandstone.webp`,
    );
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.wrapS = texture.wrapT = THREE.MirroredRepeatWrapping;
    texture.anisotropy = 4;
    stone = new THREE.MeshStandardMaterial({
      map: texture,
      bumpMap: texture,
      bumpScale: 0.075,
      roughness: 0.97,
      color: 0xffffff,
    });
  }
  return stone;
}

/** Low-polygon ledges and fractured caps share their exact shape with collision. */
export function sandstoneRock(w: number, h: number, d: number, variant = 0): THREE.Mesh {
  const key = `${w}/${h}/${d}/${variant}`;
  let geometry = geometries.get(key);
  if (!geometry) {
    const rng = new Random(812 + variant);
    const shape = quarryRockShape(w, h, d, variant);
    const vertices = Array.from({ length: shape.positions.length / 3 }, (_, i) =>
      new THREE.Vector3().fromArray(shape.positions, i * 3),
    );
    const positions: number[] = [];
    const uvs: number[] = [];
    const colors: number[] = [];
    const triangle = (a: number, b: number, c: number, face: number, shade: number) => {
      for (const index of [a, b, c]) {
        const p = vertices[index];
        positions.push(p.x, p.y, p.z);
        uvs.push((face === 1 ? p.z : p.x) / 5, (face === 2 ? p.z : p.y) / 5);
        colors.push(shade, shade * 0.98, shade * 0.94);
      }
    };
    for (let ring = 0; ring < 4; ring++) {
      for (let side = 0; side < 8; side++) {
        const a = ring * 8 + side;
        const b = ring * 8 + ((side + 1) % 8);
        const shade = (ring === 2 ? 0.86 : 0.97) * rng.range(0.94, 1);
        const face = side === 2 || side === 6 ? 1 : 0;
        triangle(a, a + 8, b, face, shade);
        triangle(b, a + 8, b + 8, face, shade);
      }
    }
    for (let i = 1; i < 7; i++) {
      triangle(32, 32 + i + 1, 32 + i, 2, 1);
    }
    geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
    geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
    geometry.computeVertexNormals();
    geometries.set(key, geometry);
  }
  const mat = sandstoneMaterial();
  mat.vertexColors = true;
  const mesh = new THREE.Mesh(geometry, mat);
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}
