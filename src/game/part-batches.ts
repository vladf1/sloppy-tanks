import * as THREE from "three/webgpu";
import {
  Fn,
  attribute,
  storage,
  normalLocal,
  positionLocal,
  transformNormal,
  uint,
  vec4,
} from "three/tsl";
import { mergeGeometries } from "three/addons/utils/BufferGeometryUtils.js";
import { isMesh } from "./render-resources";

const hidden = new THREE.Matrix4().makeTranslation(0, -1e6, 0);
interface Part {
  mesh: THREE.Mesh<THREE.BufferGeometry, THREE.Material>;
  layers: number;
}
interface DrawBatch {
  mesh: THREE.Mesh<THREE.BufferGeometry, THREE.NodeMaterial>;
  parts: Part[];
  transforms: Float32Array;
}

function nodeMaterial(source: THREE.Material): THREE.NodeMaterial | undefined {
  let material: THREE.NodeMaterial;
  if (source instanceof THREE.NodeMaterial) {
    if (source.positionNode || source.vertexNode) {
      return undefined;
    }
    material = source.clone();
  } else if (source instanceof THREE.MeshStandardMaterial) {
    material = new THREE.MeshStandardNodeMaterial().copy(source);
  } else if (source instanceof THREE.MeshBasicMaterial) {
    material = new THREE.MeshBasicNodeMaterial().copy(source);
  } else {
    return undefined;
  }
  // NodeMaterial.copy() in r185 skips these inherited accessors.
  material.alphaTest = source.alphaTest;
  material.alphaToCoverage = source.alphaToCoverage;
  material.userData = { ...material.userData, owned: true };
  return material;
}

/** Merge opaque parts into a few draws, while the existing model hierarchy owns
 * every pose and visibility decision. GPU matrices preserve suspension, recoil,
 * moving cover, tree boughs and respawns without baking animation into geometry. */
export class PartBatches {
  readonly group = new THREE.BundleGroup();
  readonly batches: DrawBatch[] = [];
  private inverse = new THREE.Matrix4();
  private relative = new THREE.Matrix4();
  private poses?: THREE.StorageBufferAttribute;
  private matrices?: THREE.StorageBufferNode<"mat4">;
  private offset = 0;

  constructor(private releaseStorage?: (attribute: THREE.StorageBufferAttribute) => void) {}

  rebuild(roots: Iterable<THREE.Object3D>): void {
    this.dispose();
    const buckets = new Map<string, Part[]>();
    for (const root of roots) {
      root.traverse((object) => {
        if (
          !isMesh(object) ||
          object instanceof THREE.InstancedMesh ||
          Array.isArray(object.material) ||
          object.material.transparent ||
          object.geometry.groups.length > 1 ||
          object.geometry.drawRange.start !== 0 ||
          object.geometry.drawRange.count !== Infinity ||
          Object.keys(object.geometry.morphAttributes).length > 0
        ) {
          return;
        }
        const material = object.material;
        if (!(
          material instanceof THREE.MeshStandardMaterial ||
          material instanceof THREE.MeshBasicMaterial ||
          material instanceof THREE.NodeMaterial
        )) {
          return;
        }
        if (
          material instanceof THREE.NodeMaterial &&
          (material.positionNode || material.vertexNode)
        ) {
          return;
        }
        const attributes = Object.entries(object.geometry.attributes).sort(([a], [b]) =>
          a.localeCompare(b),
        );
        if (attributes.some(([, a]) => a instanceof THREE.InstancedBufferAttribute)) {
          return;
        }
        const layout = attributes.map(([name, a]) =>
          [name, a.itemSize, a.normalized, a.array.constructor.name].join(":"),
        );
        const key = [
          material.uuid,
          object.castShadow,
          object.receiveShadow,
          object.renderOrder,
          object.layers.mask,
          !!object.geometry.index,
          ...layout,
        ].join("/");
        const parts = buckets.get(key) ?? [];
        parts.push({
          mesh: object as THREE.Mesh<THREE.BufferGeometry, THREE.Material>,
          layers: object.layers.mask,
        });
        buckets.set(key, parts);
      });
    }
    const count = [...buckets.values()].reduce(
      (sum, parts) => sum + (parts.length > 1 ? parts.length : 0),
      0,
    );
    if (!count) {
      return;
    }
    this.poses = new THREE.StorageBufferAttribute(count, 16);
    // Version-gated uploads: r185's DynamicDrawUsage uploads again for every
    // material/pass that reads this buffer, even when its version is unchanged.
    this.matrices = storage(this.poses, "mat4", count).toReadOnly();
    for (const parts of buckets.values()) {
      if (parts.length > 1) {
        this.add(parts);
      }
    }
  }

  private add(parts: Part[]): void {
    const first = parts[0].mesh;
    const material = nodeMaterial(first.material);
    if (!material) {
      return;
    }
    const geometries = parts.map(({ mesh }, i) => {
      const geometry = mesh.geometry.clone();
      geometry.setAttribute(
        "partIndex",
        new THREE.Float32BufferAttribute(
          new Float32Array(geometry.getAttribute("position").count).fill(this.offset + i),
          1,
        ),
      );
      return geometry;
    });
    const geometry = mergeGeometries(geometries);
    for (const temporary of geometries) {
      temporary.dispose();
    }
    if (!geometry) {
      material.dispose();
      return;
    }
    const transforms = (this.poses!.array as Float32Array).subarray(
      this.offset * 16,
      (this.offset + parts.length) * 16,
    );
    this.offset += parts.length;
    const matrix = this.matrices!.element(uint(attribute("partIndex", "float" as const)));
    material.positionNode = Fn(() => {
      normalLocal.assign(transformNormal(normalLocal, matrix));
      return matrix.mul(vec4(positionLocal, 1)).xyz;
    })();
    const mesh = new THREE.Mesh(geometry, material);
    mesh.name = "model-parts-batch";
    mesh.castShadow = first.castShadow;
    mesh.receiveShadow = first.receiveShadow;
    mesh.renderOrder = first.renderOrder;
    mesh.layers.mask = parts[0].layers;
    // Bounds depend on GPU matrices. The arena is bounded; retain every source
    // triangle and let the GPU clip it instead of culling against unposed bounds.
    mesh.frustumCulled = false;
    geometry.userData.owned = true;
    for (const part of parts) {
      part.mesh.userData.batchedLayers = part.layers;
      part.mesh.layers.mask = 0;
    }
    this.batches.push({ mesh, parts, transforms });
    this.group.add(mesh);
  }

  /** Call after the scene updates world matrices and before submitting any pass. */
  update(): void {
    this.inverse.copy(this.group.matrixWorld).invert();
    for (const batch of this.batches) {
      for (let i = 0; i < batch.parts.length; i++) {
        const source = batch.parts[i].mesh;
        let visible = source.material.visible;
        for (
          let ancestor: THREE.Object3D | null = source;
          visible && ancestor;
          ancestor = ancestor.parent
        ) {
          visible = ancestor.visible;
        }
        if (visible) {
          this.relative
            .multiplyMatrices(this.inverse, source.matrixWorld)
            .toArray(batch.transforms, i * 16);
        } else {
          hidden.toArray(batch.transforms, i * 16);
        }
      }
    }
    // One shared storage upload serves every material, shadow and reflection pass.
    if (this.poses) {
      this.poses.needsUpdate = true;
    }
  }

  /** Release only what batching created: merged geometry, copied materials and the
   * shared pose buffer. Source models keep their resources and regain their layers. */
  dispose(): void {
    this.group.needsUpdate = true;
    for (const batch of this.batches) {
      for (const part of batch.parts) {
        part.mesh.layers.mask = part.layers;
        delete part.mesh.userData.batchedLayers;
      }
      batch.mesh.geometry.dispose();
      batch.mesh.material.dispose();
    }
    this.batches.length = 0;
    this.group.clear();
    if (this.poses) {
      this.releaseStorage?.(this.poses);
    }
    this.poses = undefined;
    this.matrices = undefined;
    this.offset = 0;
  }
}
