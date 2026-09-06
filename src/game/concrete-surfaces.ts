import * as THREE from "three";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";

const geometries = new Map<string, THREE.BufferGeometry>();
let material: THREE.MeshStandardMaterial | undefined;

/** One saved image shared by every perimeter wall; UVs repeat every four metres. */
export function concreteWall(w: number, h: number, d: number) {
  if (!material) {
    const texture = new THREE.TextureLoader().load(
      `${import.meta.env?.BASE_URL ?? "/"}textures/walls/weathered-concrete.webp`,
    );
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.wrapS = texture.wrapT = THREE.RepeatWrapping;
    texture.anisotropy = 4;
    material = new THREE.MeshStandardMaterial({
      map: texture, bumpMap: texture, bumpScale: 0.035,
      roughness: 0.95, metalness: 0,
    });
  }
  const key = `${w}/${h}/${d}`;
  let geometry = geometries.get(key);
  if (!geometry) {
    geometry = new RoundedBoxGeometry(w, h, d, 1, 0.06);
    const positions = geometry.getAttribute("position");
    const normals = geometry.getAttribute("normal"), uv = geometry.getAttribute("uv");
    for (let i = 0; i < positions.count; i++) {
      const x = positions.getX(i), y = positions.getY(i), z = positions.getZ(i);
      const nx = Math.abs(normals.getX(i)), ny = Math.abs(normals.getY(i));
      const nz = Math.abs(normals.getZ(i));
      // World-sized projection covers long faces, narrow ends and top surfaces.
      uv.setXY(i, (nx > ny && nx > nz ? z : x) / 4,
        (ny >= nx && ny >= nz ? z : y) / 4);
    }
    geometries.set(key, geometry);
  }
  const mesh = new THREE.Mesh(geometry, material);
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}
