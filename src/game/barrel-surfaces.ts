import * as THREE from "three";

let surface: THREE.MeshStandardMaterial | undefined;
let geometry: THREE.CylinderGeometry | undefined;

/** One shared offline atlas and the same twelve-sided cylinder as before. */
export function explosiveBarrel() {
  if (!surface) {
    const map = new THREE.TextureLoader().load(
      `${import.meta.env?.BASE_URL ?? "/"}textures/barrels/painted-drum.png`,
    );
    map.colorSpace = THREE.SRGBColorSpace;
    map.anisotropy = 4;
    surface = new THREE.MeshStandardMaterial({ map, roughness: 0.82, metalness: 0.15 });
  }
  if (!geometry) {
    geometry = new THREE.CylinderGeometry(0.6, 0.6, 1.6, 12);
    const uv = geometry.getAttribute("uv");
    const normal = geometry.getAttribute("normal");
    for (let i = 0; i < uv.count; i++) {
      const u = uv.getX(i);
      const v = uv.getY(i);
      if (Math.abs(normal.getY(i)) > 0.5) {
        uv.setXY(i, 0.75 + u * 0.25, 0.25 + v * 0.5);
      } else {
        uv.setXY(i, u * 0.75, v);
      }
    }
  }
  const mesh = new THREE.Mesh(geometry, surface);
  mesh.castShadow = mesh.receiveShadow = true;
  return mesh;
}
