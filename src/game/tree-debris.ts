import * as THREE from "three";
import { debrisCleanupProgress } from "./debris-cleanup";
import { isMesh, restoreBatchedLayers } from "./render-resources";

const MAX_BRANCHES = 32;
const LIFETIME = 6;
type FallingBranch = {
  model: THREE.Group;
  velocity: THREE.Vector3;
  spin: THREE.Vector3;
  scale: THREE.Vector3;
  life: number;
  landed: boolean;
  restingY: number;
  materials: THREE.Material[];
};

/** Cosmetic boughs borrow geometry and textures, with owned materials for fading. */
export class TreeDebris {
  readonly group = new THREE.Group();
  readonly branches: FallingBranch[] = [];
  private center = new THREE.Vector3();

  readonly shed = (source: THREE.Group): void => {
    if (this.branches.length === MAX_BRANCHES) {
      this.remove(this.branches.shift()!);
    }
    const { model, materials } = this.branchModel(source);
    source.parent!.getWorldPosition(this.center);
    const outward = model.position.clone().sub(this.center).setY(0).normalize();
    this.group.add(model);
    this.branches.push({
      model,
      velocity: outward.multiplyScalar(1.2 + Math.random()).setY(-0.5),
      spin: new THREE.Vector3(Math.random() - 0.5, Math.random() - 0.5, Math.random() - 0.5),
      scale: model.scale.clone(),
      life: LIFETIME,
      landed: false,
      restingY: 0,
      materials,
    });
  };

  /** A detached, fade-ready copy of a bough; also built by the shader warm-up. */
  branchModel(source: THREE.Group): { model: THREE.Group; materials: THREE.Material[] } {
    const model = source.clone(true);
    restoreBatchedLayers(model);
    source.updateWorldMatrix(true, false);
    source.matrixWorld.decompose(model.position, model.quaternion, model.scale);
    model.matrixAutoUpdate = true;
    const materials: THREE.Material[] = [];
    model.traverse((object) => {
      object.matrixWorldAutoUpdate = true;
      if (isMesh(object)) {
        const clone = (source: THREE.Material) => {
          const material = source.clone();
          material.transparent = true;
          material.depthWrite = false;
          materials.push(material);
          return material;
        };
        object.material = Array.isArray(object.material)
          ? object.material.map(clone)
          : clone(object.material);
      }
    });
    model.visible = true;
    return { model, materials };
  }

  update(dt: number): void {
    for (let i = this.branches.length - 1; i >= 0; i--) {
      const branch = this.branches[i];
      const { model, velocity, spin, scale } = branch;
      branch.life -= dt;
      if (branch.life <= 0) {
        this.remove(branch);
        this.branches.splice(i, 1);
        continue;
      }
      if (!branch.landed) {
        velocity.y -= 9.8 * dt;
        model.position.addScaledVector(velocity, dt);
        model.rotateX(spin.x * dt * 3);
        model.rotateY(spin.y * dt * 3);
        model.rotateZ(spin.z * dt * 3);
        model.updateMatrixWorld(true);
        const bottom = new THREE.Box3().setFromObject(model).min.y;
        if (bottom <= 0.03) {
          model.position.y += 0.03 - bottom;
          branch.restingY = model.position.y;
          branch.landed = true;
        }
      }
      const cleanup = debrisCleanupProgress(branch.life);
      model.scale.copy(scale);
      for (const material of branch.materials) {
        material.opacity = 1 - cleanup;
      }
      if (branch.landed) {
        model.position.y = branch.restingY - cleanup * 0.6;
        model.updateMatrixWorld(true);
      }
    }
  }

  reset(): void {
    for (const branch of this.branches) {
      this.remove(branch);
    }
    this.group.clear();
    this.branches.length = 0;
  }

  private remove(branch: FallingBranch): void {
    this.group.remove(branch.model);
    for (const material of branch.materials) {
      material.dispose();
    }
  }
}
