import * as THREE from "three";
import type { PickupKind } from "./types";

const cubeGeometry = new THREE.BoxGeometry(1.25, 1.25, 1.25);
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
  const cube = new THREE.Mesh(cubeGeometry, faceMaterial(kind));
  cube.castShadow = true;
  return cube;
}
