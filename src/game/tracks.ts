import * as THREE from "three";
import { VEHICLES, angleDelta } from "./data";
import type { Simulation } from "./simulation";

export const TRACK_CAPACITY = 8192;
export const TRACK_LIFETIME = 18;
const SPACING = 0.42;
interface Pose { x: number; z: number; heading: number; pending: number }

/** Cosmetic, distance-spaced twin treads in one bounded draw call. */
export class TrackTrails {
  mesh: THREE.InstancedMesh;
  private birth = new THREE.InstancedBufferAttribute(new Float32Array(TRACK_CAPACITY), 1);
  private clock = { value: 0 };
  private poses = new Map<number, Pose>();
  private cursor = 0;
  private dummy = new THREE.Object3D();

  constructor() {
    const geometry = new THREE.PlaneGeometry(1, 1).rotateX(-Math.PI / 2);
    geometry.setAttribute("trackBirth", this.birth);
    const material = new THREE.MeshBasicMaterial({
      color: 0x283222, transparent: true, opacity: 0.38, depthWrite: false,
      polygonOffset: true, polygonOffsetFactor: -1, polygonOffsetUnits: -1,
    });
    material.onBeforeCompile = (shader) => {
      shader.uniforms.trackTime = this.clock;
      shader.vertexShader = `attribute float trackBirth;\nvarying float treadAge;\nuniform float trackTime;\n${shader.vertexShader}`
        .replace("#include <begin_vertex>", "#include <begin_vertex>\ntreadAge = trackTime - trackBirth;");
      shader.fragmentShader = `varying float treadAge;\n${shader.fragmentShader}`
        .replace("#include <color_fragment>", `#include <color_fragment>\ndiffuseColor.a *= 1.0 - smoothstep(4.0, ${TRACK_LIFETIME.toFixed(1)}, treadAge);`);
    };
    material.customProgramCacheKey = () => "tank-tread-fade-v1";
    this.mesh = new THREE.InstancedMesh(geometry, material, TRACK_CAPACITY);
    this.mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.birth.setUsage(THREE.DynamicDrawUsage);
    this.mesh.count = 0;
    this.mesh.frustumCulled = false;
    this.mesh.renderOrder = 1;
  }

  reset() {
    this.mesh.count = 0;
    this.cursor = 0;
    this.poses.clear();
    this.mesh.instanceMatrix.clearUpdateRanges();
    this.birth.clearUpdateRanges();
    this.clock.value = 0;
  }

  update(s: Simulation, alpha: number) {
    this.clock.value = s.elapsed;
    const first = this.cursor;
    let written = 0;
    for (const t of s.tanks) {
      if (!t.alive) { this.poses.delete(t.id); continue; }
      const p = t.body.translation();
      const x = THREE.MathUtils.lerp(t.previous.x, p.x, alpha);
      const z = THREE.MathUtils.lerp(t.previous.z, p.z, alpha);
      const previous = this.poses.get(t.id);
      if (!previous) {
        this.poses.set(t.id, { x, z, heading: t.heading, pending: 0 });
        continue;
      }
      const length = Math.hypot(x - previous.x, z - previous.z);
      const scale = VEHICLES[t.kind].scale;
      const spacing = SPACING * scale;
      if (length > 5 || p.y > 1.25) previous.pending = 0;
      else if (length > 1e-6) {
        // Subdivide travel so marks remain evenly spaced at different frame rates.
        for (let d = spacing - previous.pending; d <= length; d += spacing) {
          const u = d / length;
          const angle = previous.heading + angleDelta(previous.heading, t.heading) * u;
          const sin = Math.sin(angle), cos = Math.cos(angle);
          const cx = previous.x + (x - previous.x) * u - sin * 1.1 * scale;
          const cz = previous.z + (z - previous.z) * u - cos * 1.1 * scale;
          // Never overwrite a visible tread when traffic fills the ring buffer.
          // Skip this pair until its oldest slot has finished fading instead.
          if (this.mesh.count === TRACK_CAPACITY &&
              s.elapsed - this.birth.getX(this.cursor) < TRACK_LIFETIME) continue;
          for (const side of [-1, 1]) {
            this.dummy.position.set(cx + cos * side * scale, 0.075, cz - sin * side * scale);
            this.dummy.rotation.set(0, angle, 0);
            this.dummy.scale.set(0.48 * scale, 1, 0.16 * scale);
            this.dummy.updateMatrix();
            this.mesh.setMatrixAt(this.cursor, this.dummy.matrix);
            written++;
            this.birth.setX(this.cursor, s.elapsed);
            this.cursor = (this.cursor + 1) % TRACK_CAPACITY;
            this.mesh.count = Math.min(TRACK_CAPACITY, this.mesh.count + 1);
          }
        }
        previous.pending = (previous.pending + length) % spacing;
      }
      previous.x = x; previous.z = z; previous.heading = t.heading;
    }
    if (written) {
      // A wrapped ring touches at most two contiguous ranges, not the full buffer.
      const addRange = (start: number, count: number) => {
        this.mesh.instanceMatrix.addUpdateRange(start * 16, count * 16);
        this.birth.addUpdateRange(start, count);
      };
      if (written >= TRACK_CAPACITY) addRange(0, TRACK_CAPACITY);
      else {
        const tail = Math.min(written, TRACK_CAPACITY - first);
        addRange(first, tail);
        if (written > tail) addRange(0, written - tail);
      }
      this.mesh.instanceMatrix.needsUpdate = true;
      this.birth.needsUpdate = true;
    }
  }

  dispose() {
    this.mesh.geometry.dispose();
    (this.mesh.material as THREE.Material).dispose();
    this.mesh.dispose();
  }
}
