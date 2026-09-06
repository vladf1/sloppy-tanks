import * as THREE from "three";
import { mergeGeometries } from "three/addons/utils/BufferGeometryUtils.js";

const coloredMaterials = new Map<string, THREE.MeshStandardMaterial>();

/** The generated models use opaque, untextured standard materials. Bake their
 * diffuse colors into vertices while keeping each distinct lighting response. */
function vertexMaterial(source: THREE.Material) {
  if (
    !(source instanceof THREE.MeshStandardMaterial) ||
    source.map || source.normalMap || source.roughnessMap || source.metalnessMap ||
    source.alphaMap || source.aoMap || source.lightMap || source.emissiveMap ||
    source.envMap || source.bumpMap || source.displacementMap ||
    source.transparent || source.opacity !== 1 || source.alphaTest ||
    source.vertexColors || source.wireframe ||
    source.emissive.getHex() !== 0
  ) return source;
  const key = [source.metalness, source.roughness, source.toneMapped,
    source.side, source.flatShading, source.depthTest, source.depthWrite].join("/");
  let mat = coloredMaterials.get(key);
  if (!mat) {
    mat = source.clone();
    mat.color.setHex(0xffffff);
    mat.vertexColors = true;
    coloredMaterials.set(key, mat);
  }
  return mat;
}

/** Batch only direct mesh children, preserving movable assembly groups. */
export function batch(group: THREE.Group) {
  group.updateMatrixWorld(true);
  const byMat = new Map<THREE.Material, THREE.BufferGeometry[]>();
  for (const child of [...group.children]) {
    if (!(child instanceof THREE.Mesh) || Array.isArray(child.material)) continue;
    const source = child.material as THREE.Material;
    const mat = vertexMaterial(source);
    const geo = (child.geometry.index
      ? child.geometry.toNonIndexed()
      : child.geometry.clone()).applyMatrix4(child.matrix);
    if (mat !== source) {
      const color = (source as THREE.MeshStandardMaterial).color;
      const colors = new Float32Array(geo.getAttribute("position").count * 3);
      for (let i = 0; i < colors.length; i += 3) {
        colors[i] = color.r;
        colors[i + 1] = color.g;
        colors[i + 2] = color.b;
      }
      geo.setAttribute("color", new THREE.BufferAttribute(colors, 3));
    }
    const list = byMat.get(mat) ?? [];
    list.push(geo);
    byMat.set(mat, list);
    group.remove(child);
  }
  for (const [mat, geos] of byMat) {
    const geo = mergeGeometries(geos);
    if (geo) {
      const mesh = new THREE.Mesh(geo, mat);
      mesh.castShadow = mesh.receiveShadow = true;
      // Its vertices already contain the local transform; parent assemblies move.
      mesh.matrixAutoUpdate = false;
      mesh.matrixWorldNeedsUpdate = true;
      geo.userData.owned = true;
      group.add(mesh);
    }
    for (const geo of geos) geo.dispose();
  }
}

/** Use only for scenery whose ancestors and local transforms remain stationary. */
export function freezeStatic(group: THREE.Object3D) {
  group.updateMatrixWorld(true);
  group.traverse(object => {
    object.matrixAutoUpdate = false;
    object.matrixWorldAutoUpdate = false;
  });
}
