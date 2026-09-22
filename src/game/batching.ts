import * as THREE from "three";
import { interleaveAttributes, mergeGeometries } from "three/addons/utils/BufferGeometryUtils.js";
import { isMesh } from "./render-resources";

const coloredMaterials = new Map<string, THREE.MeshStandardMaterial>();

/** Bake opaque paint colors into vertices; identical color/bump maps can share
 * a batch while retaining their UVs and distinct lighting responses. */
function vertexMaterial(source: THREE.Material) {
  if (
    !(source instanceof THREE.MeshStandardMaterial) ||
    source.normalMap ||
    source.roughnessMap ||
    source.metalnessMap ||
    source.alphaMap ||
    source.aoMap ||
    source.lightMap ||
    source.emissiveMap ||
    source.envMap ||
    source.displacementMap ||
    source.transparent ||
    source.opacity !== 1 ||
    source.alphaTest ||
    source.vertexColors ||
    source.wireframe ||
    source.emissive.getHex() !== 0
  ) {
    return source;
  }
  const key = [
    source.metalness,
    source.roughness,
    source.toneMapped,
    source.side,
    source.flatShading,
    source.depthTest,
    source.depthWrite,
    source.map?.uuid,
    source.bumpMap?.uuid,
    source.bumpScale,
  ].join("/");
  let mat = coloredMaterials.get(key);
  if (!mat) {
    mat = source.clone();
    mat.color.setHex(0xffffff);
    mat.vertexColors = true;
    coloredMaterials.set(key, mat);
  }
  return mat;
}

export interface BatchPart {
  mesh: THREE.Mesh;
  /** Paint baked into a new color attribute when the batch uses a vertex material. */
  color?: THREE.Color;
}

const vertex = new THREE.Vector3();
const normalMatrix = new THREE.Matrix3();

/** Transform, de-index, merge and interleave in one pass, writing each vertex once.
 * Equivalent to toNonIndexed/applyMatrix4/mergeGeometries/interleaveAttributes,
 * which copied every attribute several times while the arena loaded. Returns
 * undefined for layouts outside the plain or interleaved Float32 case; batch()
 * then uses the general utilities. */
export function packParts(parts: BatchPart[]): THREE.BufferGeometry | undefined {
  const paint = parts[0].color !== undefined;
  const names = Object.keys(parts[0].mesh.geometry.attributes);
  if (paint && !names.includes("color")) {
    names.push("color");
  }
  const sizes = names.map((name) =>
    paint && name === "color" ? 3 : parts[0].mesh.geometry.getAttribute(name).itemSize,
  );
  let vertices = 0;
  for (const { mesh } of parts) {
    const geometry = mesh.geometry;
    const own = Object.keys(geometry.attributes);
    const count = geometry.getAttribute("position")?.count;
    if (
      Object.keys(geometry.morphAttributes).length > 0 ||
      own.length + (paint && !own.includes("color") ? 1 : 0) !== names.length
    ) {
      return undefined;
    }
    for (const [i, name] of names.entries()) {
      if (paint && name === "color") {
        continue;
      }
      const attribute = geometry.attributes[name];
      const array =
        attribute instanceof THREE.BufferAttribute ? attribute.array : attribute.data.array;
      if (
        !(array instanceof Float32Array) ||
        attribute.normalized ||
        attribute.itemSize !== sizes[i] ||
        attribute.count !== count ||
        (["position", "normal"].includes(name) && attribute.itemSize !== 3) ||
        (name === "tangent" && attribute.itemSize < 3)
      ) {
        return undefined;
      }
    }
    vertices += geometry.index ? geometry.index.count : count;
  }
  const stride = sizes.reduce((sum, size) => sum + size, 0);
  const packed = new Float32Array(vertices * stride);
  let base = 0;
  for (const { mesh, color } of parts) {
    const geometry = mesh.geometry;
    const index = geometry.index?.array;
    const count = index ? index.length : geometry.getAttribute("position").count;
    normalMatrix.getNormalMatrix(mesh.matrix);
    let offset = 0;
    for (const [i, name] of names.entries()) {
      const size = sizes[i];
      if (color && name === "color") {
        for (let v = 0, out = base + offset; v < count; v++, out += stride) {
          packed[out] = color.r;
          packed[out + 1] = color.g;
          packed[out + 2] = color.b;
        }
        offset += size;
        continue;
      }
      const attribute = geometry.attributes[name];
      // Already batched parts (tree assemblies) arrive interleaved.
      const [source, step, start] =
        attribute instanceof THREE.BufferAttribute
          ? [attribute.array as Float32Array, size, 0]
          : [attribute.data.array as Float32Array, attribute.data.stride, attribute.offset];
      // BufferGeometry.applyMatrix4 transforms exactly these three attributes.
      const transform = ["position", "normal", "tangent"].indexOf(name);
      for (let v = 0, out = base + offset; v < count; v++, out += stride) {
        const at = (index ? index[v] : v) * step + start;
        if (transform >= 0) {
          vertex.fromArray(source, at);
          if (transform === 0) {
            vertex.applyMatrix4(mesh.matrix);
          } else if (transform === 1) {
            vertex.applyNormalMatrix(normalMatrix);
          } else {
            vertex.transformDirection(mesh.matrix);
          }
          packed[out] = vertex.x;
          packed[out + 1] = vertex.y;
          packed[out + 2] = vertex.z;
          for (let k = 3; k < size; k++) {
            packed[out + k] = source[at + k];
          }
        } else {
          for (let k = 0; k < size; k++) {
            packed[out + k] = source[at + k];
          }
        }
      }
      offset += size;
    }
    base += count * stride;
  }
  // These baked vertices never change. One interleaved buffer avoids rebinding
  // separate position/normal/UV/color buffers for each WebGPU draw.
  const buffer = new THREE.InterleavedBuffer(packed, stride);
  const geometry = new THREE.BufferGeometry();
  let offset = 0;
  for (const [i, name] of names.entries()) {
    geometry.setAttribute(name, new THREE.InterleavedBufferAttribute(buffer, sizes[i], offset));
    offset += sizes[i];
  }
  return geometry;
}

/** The general path for layouts packParts() does not handle. */
export function mergeParts(parts: BatchPart[]): THREE.BufferGeometry | null {
  const geos = parts.map(({ mesh, color }) => {
    const geo = (
      mesh.geometry.index ? mesh.geometry.toNonIndexed() : mesh.geometry.clone()
    ).applyMatrix4(mesh.matrix);
    if (color) {
      const colors = new Float32Array(geo.getAttribute("position").count * 3);
      for (let i = 0; i < colors.length; i += 3) {
        colors[i] = color.r;
        colors[i + 1] = color.g;
        colors[i + 2] = color.b;
      }
      geo.setAttribute("color", new THREE.BufferAttribute(colors, 3));
    }
    return geo;
  });
  const geo = mergeGeometries(geos);
  for (const part of geos) {
    part.dispose();
  }
  if (geo) {
    const entries = Object.entries(geo.attributes).filter(
      (entry): entry is [string, THREE.BufferAttribute] =>
        entry[1] instanceof THREE.BufferAttribute,
    );
    // r185's declaration omits the array in this utility's return type.
    const packed = interleaveAttributes(entries.map(([, attribute]) => attribute)) as unknown as
      THREE.InterleavedBufferAttribute[] | null;
    if (packed) {
      entries.forEach(([name], i) => geo.setAttribute(name, packed[i]));
    }
  }
  return geo;
}

/** Group a batch's direct mesh children by their final draw material. */
export function batchParts(group: THREE.Group): Map<THREE.Material, BatchPart[]> {
  group.updateMatrixWorld(true);
  const byMat = new Map<THREE.Material, BatchPart[]>();
  for (const child of group.children) {
    if (!isMesh(child) || Array.isArray(child.material)) {
      continue;
    }
    const source = child.material;
    const mat = vertexMaterial(source);
    const list = byMat.get(mat) ?? [];
    list.push({
      mesh: child,
      color: mat !== source ? (source as THREE.MeshStandardMaterial).color : undefined,
    });
    byMat.set(mat, list);
  }
  return byMat;
}

/** Batch only direct mesh children, preserving movable assembly groups. */
export function batch(group: THREE.Group): void {
  const byMat = batchParts(group);
  for (const parts of byMat.values()) {
    group.remove(...parts.map((part) => part.mesh));
  }
  for (const [mat, parts] of byMat) {
    const geo = packParts(parts) ?? mergeParts(parts);
    if (geo) {
      const mesh = new THREE.Mesh(geo, mat);
      mesh.castShadow = mesh.receiveShadow = true;
      // Its vertices already contain the local transform; parent assemblies move.
      mesh.matrixAutoUpdate = false;
      mesh.matrixWorldNeedsUpdate = true;
      geo.userData.owned = true;
      group.add(mesh);
    }
  }
}

/** Use only for scenery whose ancestors and local transforms remain stationary. */
export function freezeStatic(group: THREE.Object3D): void {
  group.updateMatrixWorld(true);
  group.traverse((object) => {
    object.matrixAutoUpdate = false;
    object.matrixWorldAutoUpdate = false;
  });
}
