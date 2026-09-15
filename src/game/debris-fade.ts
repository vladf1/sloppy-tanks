import * as THREE from "three";

/** Per-piece opacity keeps debris batched while it sinks and fades smoothly. */
export function addDebrisFade(mesh: THREE.InstancedMesh): void {
  mesh.geometry = mesh.geometry.clone();
  mesh.geometry.computeBoundingBox();
  const fade = new THREE.InstancedBufferAttribute(new Float32Array(mesh.instanceMatrix.count), 1);
  fade.setUsage(THREE.DynamicDrawUsage);
  mesh.geometry.setAttribute("debrisOpacity", fade);
  const configure = (material: THREE.Material, shadow = false) => {
    material.alphaHash = shadow;
    material.transparent = !shadow;
    material.depthWrite = shadow;
    material.onBeforeCompile = (shader) => {
      shader.vertexShader =
        `attribute float debrisOpacity;\nvarying float vDebrisOpacity;\n${shader.vertexShader}`.replace(
          "#include <begin_vertex>",
          "#include <begin_vertex>\nvDebrisOpacity = debrisOpacity;",
        );
      shader.fragmentShader = `varying float vDebrisOpacity;\n${shader.fragmentShader}`.replace(
        "#include <alphahash_fragment>",
        "diffuseColor.a *= vDebrisOpacity;\n#include <alphahash_fragment>",
      );
    };
    material.customProgramCacheKey = () => "debris-fade-v1";
    return material;
  };
  mesh.material = Array.isArray(mesh.material)
    ? mesh.material.map((material) => configure(material.clone()))
    : configure(mesh.material.clone());
  // Shadows use the same fade, so a vanished piece cannot leave a solid silhouette.
  mesh.customDepthMaterial = configure(
    new THREE.MeshDepthMaterial({ depthPacking: THREE.RGBADepthPacking }),
    true,
  );
}
