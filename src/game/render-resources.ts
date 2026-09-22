import * as THREE from "three/webgpu";

/** Persistent effect pools upload just their changed instances, once per version.
 * Ordinary r185 instance matrices below 64 KiB become full-array uniform uploads. */
export function storageInstances(mesh: THREE.InstancedMesh): void {
  if (!(mesh.instanceMatrix instanceof THREE.StorageInstancedBufferAttribute)) {
    mesh.instanceMatrix = new THREE.StorageInstancedBufferAttribute(mesh.instanceMatrix.array, 16);
  }
}

/** Detached tree pieces become independent draws, even if their source was batched. */
export function restoreBatchedLayers(group: THREE.Object3D): void {
  group.traverse((object) => {
    if (typeof object.userData.batchedLayers === "number") {
      object.layers.mask = object.userData.batchedLayers;
      delete object.userData.batchedLayers;
    }
  });
}
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
