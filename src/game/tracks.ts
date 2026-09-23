import { storageInstances } from "./render-resources";
import * as THREE from "three/webgpu";
import { attribute, uniform, smoothstep } from "three/tsl";
import { spawnPositions } from "./arena";
import { VEHICLES, angleDelta } from "./data";
import type { Simulation } from "./simulation";
import { renderState, type RenderState } from "./render-state";

// Thirty boosted scouts can leave about 70,000 marks during the 24-second fade.
// Reserve that lifetime budget so busy scenes do not stop drawing new trails.
export const TRACK_CAPACITY = 81920;
export const TRACK_LIFETIME = 24;
const SPACING = 0.42;
export const HUMVEE_TRACK_STRENGTH = 0.18;
interface Pose {
  x: number;
  z: number;
  heading: number;
  pending: number;
}

/** Cosmetic, distance-spaced twin vehicle trails in one bounded draw call. */
export class TrackTrails {
  mesh: THREE.InstancedMesh;
  private birth = new THREE.InstancedBufferAttribute(new Float32Array(TRACK_CAPACITY), 1);
  private strength = new THREE.InstancedBufferAttribute(new Float32Array(TRACK_CAPACITY), 1);
  private clock = uniform(0);
  private poses = new Map<number, Pose>();
  // Expiry order is a ring; render slots stay dense so mesh.count excludes dead marks.
  private oldest = 0;
  private slots = new Uint32Array(TRACK_CAPACITY);
  private queueIndices = new Uint32Array(TRACK_CAPACITY);
  private dummy = new THREE.Object3D();

  constructor() {
    const geometry = new THREE.PlaneGeometry(1, 1).rotateX(-Math.PI / 2);
    geometry.setAttribute("trackBirth", this.birth);
    geometry.setAttribute("trackStrength", this.strength);
    const material = new THREE.MeshBasicNodeMaterial({
      color: 0x283222,
      transparent: true,
      opacity: 0.38,
      depthWrite: false,
      polygonOffset: true,
      polygonOffsetFactor: -1,
      polygonOffsetUnits: -1,
    });
    material.opacityNode = attribute("trackStrength", "float" as const)
      .mul(
        smoothstep(
          4,
          TRACK_LIFETIME,
          this.clock.sub(attribute("trackBirth", "float" as const)),
        ).oneMinus(),
      )
      .mul(0.38);
    this.mesh = new THREE.InstancedMesh(geometry, material, TRACK_CAPACITY);
    storageInstances(this.mesh);

    this.mesh.count = 0;
    this.mesh.frustumCulled = false;
    this.mesh.renderOrder = 1;
  }

  /** Quarry pads stand proud of the dirt, so prints on them ride on top. */
  private markHeight(simulation: RenderState, x: number, z: number): number {
    if (simulation.mapTheme === "quarry") {
      for (const team of [0, 1] as const) {
        for (const p of spawnPositions(team)) {
          if (Math.hypot(x - p.x, z - p.z) < 2.75) {
            return 0.16;
          }
        }
      }
    }
    return 0.075;
  }

  reset(): void {
    this.mesh.count = 0;
    this.oldest = 0;
    this.poses.clear();
    this.mesh.instanceMatrix.clearUpdateRanges();
    this.birth.clearUpdateRanges();
    this.strength.clearUpdateRanges();
    this.clock.value = 0;
  }

  update(source: Simulation | RenderState, alpha: number): void {
    const simulation = renderState(source);
    // Start a fresh upload list, including when no renderer consumed the previous
    // ranges (for example during a paused/offscreen check).
    this.mesh.instanceMatrix.clearUpdateRanges();
    this.birth.clearUpdateRanges();
    this.strength.clearUpdateRanges();
    this.clock.value = simulation.elapsed;
    let changed = false;
    // Retire only expired entries, not a scan of every live mark each frame.
    // Moving the last live slot into each hole keeps a single compact draw call.
    while (this.mesh.count > 0) {
      const slot = this.slots[this.oldest];
      if (simulation.elapsed - this.birth.getX(slot) < TRACK_LIFETIME) {
        break;
      }
      const last = --this.mesh.count;
      if (slot !== last) {
        this.mesh.instanceMatrix.array.copyWithin(slot * 16, last * 16, (last + 1) * 16);
        this.birth.setX(slot, this.birth.getX(last));
        this.strength.setX(slot, this.strength.getX(last));
        const queueIndex = this.queueIndices[last];
        this.queueIndices[slot] = queueIndex;
        this.slots[queueIndex] = slot;
        this.mesh.instanceMatrix.addUpdateRange(slot * 16, 16);
        this.birth.addUpdateRange(slot, 1);
        this.strength.addUpdateRange(slot, 1);
        changed = true;
      }
      this.oldest = (this.oldest + 1) % TRACK_CAPACITY;
    }
    const first = this.mesh.count;
    for (const tank of simulation.tanks) {
      if (!tank.alive) {
        this.poses.delete(tank.id);
        continue;
      }
      const position = tank.position;
      const x = THREE.MathUtils.lerp(tank.previous.x, position.x, alpha);
      const z = THREE.MathUtils.lerp(tank.previous.z, position.z, alpha);
      const previous = this.poses.get(tank.id);
      if (!previous) {
        this.poses.set(tank.id, { x, z, heading: tank.heading, pending: 0 });
        continue;
      }
      const distance = Math.hypot(x - previous.x, z - previous.z);
      const turn = angleDelta(previous.heading, tank.heading);
      const scale = VEHICLES[tank.kind].scale;
      const humvee = tank.kind === "humvee";
      // HMMWV wheels leave overlapping narrow lines; tracked vehicles retain
      // the separated tread-pad rhythm used by the rest of the fleet.
      const spacing = (humvee ? 0.16 : SPACING) * scale;
      const length = distance + Math.abs(turn) * 1.5 * scale;
      if (distance > 5 || Math.abs(turn) > 0.8 || position.y > 1.25) {
        previous.pending = 0;
      } else if (length > 1e-6) {
        // Subdivide travel so marks remain evenly spaced at different frame rates.
        for (let d = spacing - previous.pending; d <= length; d += spacing) {
          const u = d / length;
          const angle = previous.heading + turn * u;
          const sin = Math.sin(angle);
          const cos = Math.cos(angle);
          const cx = previous.x + (x - previous.x) * u - sin * 1.1 * scale;
          const cz = previous.z + (z - previous.z) * u - cos * 1.1 * scale;
          // Capacity pressure must not erase marks before they finish fading.
          if (this.mesh.count === TRACK_CAPACITY) {
            continue;
          }
          for (const side of [-1, 1]) {
            const mx = cx + cos * side * scale;
            const mz = cz - sin * side * scale;
            this.dummy.position.set(mx, this.markHeight(simulation, mx, mz), mz);
            this.dummy.rotation.set(0, angle, 0);
            this.dummy.scale.set((humvee ? 0.18 : 0.48) * scale, 1, (humvee ? 0.3 : 0.16) * scale);
            this.dummy.updateMatrix();
            const slot = this.mesh.count++;
            const queueIndex = (this.oldest + slot) % TRACK_CAPACITY;
            this.slots[queueIndex] = slot;
            this.queueIndices[slot] = queueIndex;
            this.mesh.setMatrixAt(slot, this.dummy.matrix);
            this.birth.setX(slot, simulation.elapsed);
            this.strength.setX(slot, humvee ? HUMVEE_TRACK_STRENGTH : 1);
          }
        }
        previous.pending = (previous.pending + length) % spacing;
      }
      previous.x = x;
      previous.z = z;
      previous.heading = tank.heading;
    }
    const written = this.mesh.count - first;
    if (written) {
      this.mesh.instanceMatrix.addUpdateRange(first * 16, written * 16);
      this.birth.addUpdateRange(first, written);
      this.strength.addUpdateRange(first, written);
      changed = true;
    }
    if (changed) {
      this.mesh.instanceMatrix.needsUpdate = true;
      this.birth.needsUpdate = true;
      this.strength.needsUpdate = true;
    }
  }

  dispose(): void {
    this.mesh.geometry.dispose();
    (this.mesh.material as THREE.Material).dispose();
    this.mesh.dispose();
  }
}
