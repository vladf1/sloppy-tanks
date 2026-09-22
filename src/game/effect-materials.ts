import * as THREE from "three/webgpu";
import {
  attribute,
  Fn,
  instanceIndex,
  storage,
  cameraProjectionMatrix,
  modelViewMatrix,
  positionGeometry,
  smoothstep,
  uv,
  vec2,
  vec4,
} from "three/tsl";
import type { UniformNode } from "three/webgpu";
import { storageInstances } from "./render-resources";

/** Share the native storage matrix with standard instancing and every render pass. */
export function billboardVertex(mesh: THREE.InstancedMesh) {
  storageInstances(mesh);
  const matrix = storage(mesh.instanceMatrix, "mat4", mesh.instanceMatrix.count)
    .toReadOnly()
    .element(instanceIndex);
  const center = modelViewMatrix.mul(matrix.mul(vec4(0, 0, 0, 1)));
  const scale = vec2(
    matrix.mul(vec4(1, 0, 0, 0)).xyz.length(),
    matrix.mul(vec4(0, 1, 0, 0)).xyz.length(),
  );
  return Fn(() => {
    return cameraProjectionMatrix.mul(
      vec4(center.xy.add(positionGeometry.xy.mul(scale)), center.zw),
    );
  })();
}

export function dustMaterial(opacityAttribute: string, tint: UniformNode<"color", THREE.Color>) {
  const material = new THREE.MeshBasicNodeMaterial({
    transparent: true,
    depthWrite: false,
    fog: false,
  });
  material.colorNode = tint;
  material.opacityNode = smoothstep(0.1, 1, uv().mul(2).sub(1).length())
    .oneMinus()
    .mul(attribute(opacityAttribute, "float"));
  return material;
}
