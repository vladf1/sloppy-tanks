import * as THREE from "three/webgpu";

/** Include first-use effects and cached bundles in Three r185's precompiler.
 * Restore the exact visibility/count/culling state even if preparation fails.
 * Only presentation state changes; no simulation steps or random draws. */
export function exposeWarmupObjects(scene: THREE.Object3D): () => void {
  const restore: (() => void)[] = [];
  scene.traverse((object) => {
    const { visible, frustumCulled } = object;
    object.visible = true;
    object.frustumCulled = false;
    restore.push(() => {
      object.visible = visible;
      object.frustumCulled = frustumCulled;
    });
    if (object instanceof THREE.BundleGroup) {
      // Three types this flag readonly, but its runtime traversal uses the flag.
      const bundle = object as unknown as { isBundleGroup: boolean };
      const isBundleGroup = bundle.isBundleGroup;
      bundle.isBundleGroup = false;
      restore.push(() => {
        bundle.isBundleGroup = isBundleGroup;
        object.needsUpdate = true;
      });
    }
    if (object instanceof THREE.InstancedMesh && object.count === 0) {
      object.count = 1;
      restore.push(() => {
        object.count = 0;
      });
    }
  });
  return () => {
    for (const reset of restore) {
      reset();
    }
  };
}
