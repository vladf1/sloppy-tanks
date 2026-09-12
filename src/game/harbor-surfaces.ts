import * as THREE from "three";

// Shared albedo tiles survive resets and use real-world UV scale before batching.
const textures = new Map<string, THREE.Texture>();
const materials = new Map<string, THREE.MeshStandardMaterial>();
const geometries = new Map<string, THREE.BoxGeometry>();
export function harborMaterial(kind: "dock" | "steel", color: number) {
  const key = `${kind}/${color}`;
  let mat = materials.get(key);
  if (mat) {
    return mat;
  }
  let texture = textures.get(kind);
  if (!texture) {
    texture = new THREE.TextureLoader().load(
      `${import.meta.env?.BASE_URL ?? "/"}textures/harbor/${kind}.webp`,
    );
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.wrapS = texture.wrapT = THREE.MirroredRepeatWrapping;
    texture.anisotropy = 4;
    textures.set(kind, texture);
  }
  mat = new THREE.MeshStandardMaterial({
    color,
    map: texture,
    bumpMap: texture,
    bumpScale: kind === "dock" ? 0.035 : 0.012,
    metalness: kind === "steel" ? 0.3 : 0,
    roughness: kind === "steel" ? 0.68 : 0.95,
  });
  materials.set(key, mat);
  return mat;
}

export function harborBox(
  w: number,
  h: number,
  d: number,
  color: number,
  kind: "dock" | "steel" = "steel",
) {
  const key = `${w}/${h}/${d}/${kind}`;
  let geometry = geometries.get(key);
  if (!geometry) {
    geometry = new THREE.BoxGeometry(w, h, d);
    const p = geometry.getAttribute("position");
    const n = geometry.getAttribute("normal");
    const uv = geometry.getAttribute("uv");
    const tile = kind === "dock" ? 10 : 4;
    for (let i = 0; i < p.count; i++) {
      uv.setXY(
        i,
        (Math.abs(n.getX(i)) > 0.5 ? p.getZ(i) : p.getX(i)) / tile,
        (Math.abs(n.getY(i)) > 0.5 ? p.getZ(i) : p.getY(i)) / tile,
      );
    }
    geometries.set(key, geometry);
  }
  const mesh = new THREE.Mesh(geometry, harborMaterial(kind, color));
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}
