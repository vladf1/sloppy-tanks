import * as THREE from "three";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";
import { TEAM_COLORS } from "./data";
/** Cached geometry and materials are shared across rounds; callers own only transforms. */
const materials = new Map<string, THREE.MeshStandardMaterial>();
export function material(color: number, metalness = 0.05, roughness = 0.65) {
  const key = `${color}/${metalness}/${roughness}`;
  let m = materials.get(key);
  if (!m) {
    m = new THREE.MeshStandardMaterial({ color, metalness, roughness });
    if (TEAM_COLORS.includes(color)) {
      m.emissive.setHex(color);
      m.emissiveIntensity = 0.04;
      m.toneMapped = false;
    }
    materials.set(key, m);
  }
  return m;
}
const boxes = new Map<string, THREE.BufferGeometry>();
export function box(w: number, h: number, d: number, color: number, r = 0.06) {
  const key = [w, h, d, r].join("/");
  let geo = boxes.get(key);
  if (!geo) {
    geo = r > 0 ? new RoundedBoxGeometry(w, h, d, 1, r) : new THREE.BoxGeometry(w, h, d);
    boxes.set(key, geo);
  }
  const mesh = new THREE.Mesh(geo, material(color));
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  return mesh;
}
export function put(parent: THREE.Object3D, obj: THREE.Object3D, x = 0, y = 0, z = 0) {
  obj.position.set(x, y, z);
  parent.add(obj);
  return obj;
}
const cylinders = new Map<string, THREE.CylinderGeometry>();
export function cylinder(radius: number, height: number, color: number, sides = 12) {
  const key = `${radius}/${height}/${sides}`;
  let geo = cylinders.get(key);
  if (!geo) {
    geo = new THREE.CylinderGeometry(radius, radius, height, sides);
    cylinders.set(key, geo);
  }
  const mesh = new THREE.Mesh(geo, material(color, 0.2));
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  return mesh;
}
