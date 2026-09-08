import * as THREE from "three";
import type { PickupKind } from "./types";
import { isSpecialAmmo } from "./ammunition";
import { mergeGeometries } from "three/addons/utils/BufferGeometryUtils.js";

const cubeGeometry = new THREE.BoxGeometry(1.25, 1.25, 1.25);
const crateGeometry = new THREE.BoxGeometry(1.8, 1.05, 1.2);
const hardwareGeometry = mergeGeometries([
  // A raised rim leaves the top-face symbol visible from the overhead camera.
  new THREE.BoxGeometry(1.94, 0.14, 0.09).translate(0, 0.49, -0.615),
  new THREE.BoxGeometry(1.94, 0.14, 0.09).translate(0, 0.49, 0.615),
  new THREE.BoxGeometry(0.09, 0.14, 1.14).translate(-0.925, 0.49, 0),
  new THREE.BoxGeometry(0.09, 0.14, 1.14).translate(0.925, 0.49, 0),
  new THREE.BoxGeometry(1.9, 0.1, 1.28).translate(0, -0.5, 0),
  new THREE.BoxGeometry(0.75, 0.1, 0.12).translate(0, 0.78, 0),
  new THREE.BoxGeometry(0.12, 0.24, 0.12).translate(-0.32, 0.67, 0),
  new THREE.BoxGeometry(0.12, 0.24, 0.12).translate(0.32, 0.67, 0),
  new THREE.BoxGeometry(0.2, 0.28, 0.08).translate(0, 0.38, 0.64),
]);
const hardwareMaterial = new THREE.MeshStandardMaterial({ color: 0x273544, roughness: 0.55, metalness: 0.5 });
const faceMaterials = new Map<PickupKind, THREE.MeshStandardMaterial>();
/** Original high-contrast pictograms, shared across every face and pickup of a type. */
function faceMaterial(kind: PickupKind) {
  const cached = faceMaterials.get(kind);
  if (cached) return cached;
  const texture = new THREE.TextureLoader().load(
    `${import.meta.env.BASE_URL}textures/pickups/${kind}.png`,
  );
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = 4;
  const material = new THREE.MeshStandardMaterial({
    map: texture, roughness: 0.55, metalness: 0.15,
    emissive: 0xffffff, emissiveMap: texture, emissiveIntensity: 0.3,
    toneMapped: false,
  });
  faceMaterials.set(kind, material);
  return material;
}

export function pickupCube(kind: PickupKind) {
  if (isSpecialAmmo(kind)) {
    const crate = new THREE.Group();
    const body = new THREE.Mesh(crateGeometry, faceMaterial(kind));
    const hardware = new THREE.Mesh(hardwareGeometry, hardwareMaterial);
    body.castShadow = hardware.castShadow = true;
    crate.add(body, hardware);
    return crate;
  }
  const cube = new THREE.Mesh(cubeGeometry, faceMaterial(kind));
  cube.castShadow = true;
  return cube;
}
