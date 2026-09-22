import * as THREE from "three/webgpu";
import { attribute, hash, positionWorld } from "three/tsl";

/** Per-piece opacity keeps debris batched while it sinks and fades smoothly. */
export function addDebrisFade(mesh: THREE.InstancedMesh): void {
  mesh.geometry = mesh.geometry.clone();
  mesh.geometry.computeBoundingBox();
  const fade = new THREE.InstancedBufferAttribute(new Float32Array(mesh.instanceMatrix.count), 1);

  mesh.geometry.setAttribute("debrisOpacity", fade);
  const configure = (source: THREE.Material) => {
    const material = new THREE.MeshStandardNodeMaterial();
    // The fragments use standard surface materials; retain maps, roughness and tint.
    Object.assign(material, source.clone());
    material.transparent = true;
    material.depthWrite = false;
    material.opacityNode = attribute("debrisOpacity", "float" as const);
    // A stable spatial mask fades the shadow without an extra translucent-shadow pass.
    material.maskShadowNode = attribute("debrisOpacity", "float" as const).greaterThan(
      hash(positionWorld.mul(100).dot(positionWorld)),
    );
    return material;
  };
  mesh.material = Array.isArray(mesh.material)
    ? mesh.material.map(configure)
    : configure(mesh.material);
}
