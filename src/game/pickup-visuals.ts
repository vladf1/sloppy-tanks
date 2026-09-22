import * as THREE from "three";
import { mergeGeometries } from "three/addons/utils/BufferGeometryUtils.js";
import { isSpecialAmmo } from "./ammunition";
import type { PickupKind } from "./types";
import { PICKUP_ATLAS_PATH, pickupAtlasUV } from "./pickup-atlas";

const cubeGeometry = new THREE.BoxGeometry(1.25, 1.25, 1.25);
const crateGeometry = new THREE.BoxGeometry(1.8, 1.05, 1.2);
const hardwareGeometry = mergeGeometries([
  // A raised rim leaves the top-face symbol visible from the overhead camera.
  new THREE.BoxGeometry(1.94, 0.14, 0.09).translate(0, 0.49, -0.615),
  new THREE.BoxGeometry(1.94, 0.14, 0.09).translate(0, 0.49, 0.615),
  new THREE.BoxGeometry(0.09, 0.14, 1.14).translate(-0.925, 0.49, 0),
  new THREE.BoxGeometry(0.09, 0.14, 1.14).translate(0.925, 0.49, 0),
  new THREE.BoxGeometry(1.9, 0.1, 1.28).translate(0, -0.5, 0),
  new THREE.BoxGeometry(0.2, 0.28, 0.08).translate(0, 0.38, 0.64),
]);
const hardwareMaterial = new THREE.MeshStandardMaterial({
  color: 0x273544,
  roughness: 0.55,
  metalness: 0.5,
});
let sharedFaceMaterial: THREE.MeshStandardMaterial | undefined;
const faceGeometries = new Map<PickupKind, THREE.BufferGeometry>();
/** Original high-contrast pictograms, shared across every face and pickup of a type. */
function faceMaterial() {
  if (sharedFaceMaterial) {
    return sharedFaceMaterial;
  }
  const texture = new THREE.TextureLoader().load(`${import.meta.env.BASE_URL}${PICKUP_ATLAS_PATH}`);
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = 4;
  const material = new THREE.MeshStandardMaterial({
    map: texture,
    roughness: 0.55,
    metalness: 0.15,
    emissive: 0xffffff,
    emissiveMap: texture,
    emissiveIntensity: 0.3,
    toneMapped: false,
  });
  sharedFaceMaterial = material;
  return material;
}

function faceGeometry(kind: PickupKind) {
  let geometry = faceGeometries.get(kind);
  if (!geometry) {
    geometry = (isSpecialAmmo(kind) ? crateGeometry : cubeGeometry).clone();
    const uv = geometry.getAttribute("uv");
    for (let i = 0; i < uv.count; i++) {
      uv.setXY(i, ...pickupAtlasUV(kind, uv.getX(i), uv.getY(i)));
    }
    faceGeometries.set(kind, geometry);
  }
  return geometry;
}

export function pickupCube(kind: PickupKind) {
  if (isSpecialAmmo(kind)) {
    const crate = new THREE.Group();
    const body = new THREE.Mesh(faceGeometry(kind), faceMaterial());
    const hardware = new THREE.Mesh(hardwareGeometry, hardwareMaterial);
    body.castShadow = hardware.castShadow = true;
    crate.add(body, hardware);
    return crate;
  }
  const cube = new THREE.Mesh(faceGeometry(kind), faceMaterial());
  cube.castShadow = true;
  return cube;
}
