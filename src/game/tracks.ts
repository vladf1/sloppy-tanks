import * as THREE from "three";
import { spawnPositions } from "./arena";
import { VEHICLES, angleDelta } from "./data";
import type { Simulation } from "./simulation";

// Thirty boosted scouts can leave about 70,000 marks during the 24-second fade.
// Reserve that lifetime budget so busy scenes do not stop drawing new trails.
export const TRACK_CAPACITY = 81920;
export const TRACK_LIFETIME = 24;
const SPACING = 0.42;
interface Pose {
  x: number;
  z: number;
  heading: number;
  pending: number;
}

/** Cosmetic, distance-spaced twin treads in one bounded draw call. */
export class TrackTrails {
  mesh: THREE.InstancedMesh;
  private birth = new THREE.InstancedBufferAttribute(new Float32Array(TRACK_CAPACITY), 1);
  private clock = { value: 0 };
  private poses = new Map<number, Pose>();
  // Expiry order is a ring; render slots stay dense so mesh.count excludes dead marks.
  private oldest = 0;
  private slots = new Uint32Array(TRACK_CAPACITY);
  private queueIndices = new Uint32Array(TRACK_CAPACITY);
  private dummy = new THREE.Object3D();

  constructor() {
    const geometry = new THREE.PlaneGeometry(1, 1).rotateX(-Math.PI / 2);
    geometry.setAttribute("trackBirth", this.birth);
    const material = new THREE.MeshBasicMaterial({
      color: 0x283222,
      transparent: true,
      opacity: 0.38,
      depthWrite: false,
      polygonOffset: true,
      polygonOffsetFactor: -1,
      polygonOffsetUnits: -1,
    });
    material.onBeforeCompile = (shader) => {
      shader.uniforms.trackTime = this.clock;
      shader.vertexShader =
        `attribute float trackBirth;\nvarying float treadAge;\nuniform float trackTime;\n${shader.vertexShader}`.replace(
          "#include <begin_vertex>",
          "#include <begin_vertex>\ntreadAge = trackTime - trackBirth;",
        );
      shader.fragmentShader = `varying float treadAge;\n${shader.fragmentShader}`.replace(
        "#include <color_fragment>",
        `#include <color_fragment>\ndiffuseColor.a *= 1.0 - smoothstep(4.0, ${TRACK_LIFETIME.toFixed(1)}, treadAge);`,
      );
    };
    material.customProgramCacheKey = () => "tank-tread-fade-v1";
    this.mesh = new THREE.InstancedMesh(geometry, material, TRACK_CAPACITY);
    this.mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.birth.setUsage(THREE.DynamicDrawUsage);
    this.mesh.count = 0;
    this.mesh.frustumCulled = false;
    this.mesh.renderOrder = 1;
  }

  /** Quarry pads stand proud of the dirt, so prints on them ride on top. */
  private markHeight(simulation: Simulation, x: number, z: number): number {
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
    this.clock.value = 0;
  }

  update(simulation: Simulation, alpha: number): void {
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
        const queueIndex = this.queueIndices[last];
        this.queueIndices[slot] = queueIndex;
        this.slots[queueIndex] = slot;
        this.mesh.instanceMatrix.addUpdateRange(slot * 16, 16);
        this.birth.addUpdateRange(slot, 1);
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
      const position = tank.body.translation();
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
      const spacing = SPACING * scale;
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
            this.dummy.scale.set(0.48 * scale, 1, 0.16 * scale);
            this.dummy.updateMatrix();
            const slot = this.mesh.count++;
            const queueIndex = (this.oldest + slot) % TRACK_CAPACITY;
            this.slots[queueIndex] = slot;
            this.queueIndices[slot] = queueIndex;
            this.mesh.setMatrixAt(slot, this.dummy.matrix);
            this.birth.setX(slot, simulation.elapsed);
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
      changed = true;
    }
    if (changed) {
      this.mesh.instanceMatrix.needsUpdate = true;
      this.birth.needsUpdate = true;
    }
  }

  dispose(): void {
    this.mesh.geometry.dispose();
    (this.mesh.material as THREE.Material).dispose();
    this.mesh.dispose();
  }
}
