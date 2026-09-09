import * as THREE from "three";
/** Upload only live instance ranges and dispose only resources marked as locally owned. */
export function updateInstances(mesh: THREE.InstancedMesh): void {
  if (!mesh.count) {
    return;
  }
  mesh.instanceMatrix.clearUpdateRanges();
  mesh.instanceMatrix.addUpdateRange(0, mesh.count * 16);
  mesh.instanceMatrix.needsUpdate = true;
  if (mesh.instanceColor) {
    mesh.instanceColor.clearUpdateRanges();
    mesh.instanceColor.addUpdateRange(0, mesh.count * 3);
    mesh.instanceColor.needsUpdate = true;
  }
}
export function disposeOwned(g: THREE.Object3D): void {
  g.traverse((o) => {
    if (isMesh(o) && o.geometry.userData.owned) {
      o.geometry.dispose();
    }
    if (
      (isMesh(o) || o instanceof THREE.Sprite) &&
      !Array.isArray(o.material) &&
      (o.material as THREE.Material).userData.owned
    ) {
      (o.material as THREE.Material).dispose();
    }
  });
}

/** Three's instanceof guard defaults generic fields to any; retain their actual base types. */
export function isMesh(object: THREE.Object3D): object is THREE.Mesh {
  return object instanceof THREE.Mesh;
}
