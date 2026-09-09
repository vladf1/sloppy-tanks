import * as THREE from "three";
import { batch } from "./batching";
import { isMesh } from "./render-resources";
import { tankModel } from "./tank-model";
import type { Team, VehicleKind, WreckPart } from "./types";
// At most 3 chassis × 2 teams × 4 assemblies. Shared geometry lives across rounds.
const wreckTemplates = new Map<string, THREE.Group>();
/** Extract, center and batch once; instances share geometry but own their transforms. */
export function wreckModel(kind: VehicleKind, team: Team, part: WreckPart) {
  const key = `${kind}/${team}/${part}`;
  const cached = wreckTemplates.get(key);
  if (cached) {
    return cached.clone();
  }
  const source = tankModel(kind, team);
  const result = new THREE.Group();
  const { hull, turret, barrel } = source.userData;
  if (part === "hull") {
    result.add(hull);
  } else if (part === "barrel") {
    result.add(barrel);
  } else {
    if (part === "turret") {
      turret.remove(barrel);
    }
    result.add(turret);
  }
  const center = new THREE.Box3().setFromObject(result).getCenter(new THREE.Vector3());
  for (const child of result.children) {
    child.position.sub(center);
  }
  result.updateMatrixWorld(true);
  const meshes: THREE.Mesh[] = [];
  result.traverse((o) => {
    if (isMesh(o)) {
      meshes.push(o);
    }
  });
  const flat = new THREE.Group();
  for (const mesh of meshes) {
    mesh.applyMatrix4(mesh.parent!.matrixWorld);
    flat.add(mesh);
  }
  batch(flat);
  for (const mesh of flat.children as THREE.Mesh[]) {
    mesh.geometry.userData.owned = false;
  }
  wreckTemplates.set(key, flat);
  return flat.clone();
}
