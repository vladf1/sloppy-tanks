import * as THREE from "three";
import { VEHICLES } from "./data";
import { updateInstances } from "./render-resources";
import type { Simulation } from "./simulation";
import { isVillageDirt } from "./village-roads";

export const TRACK_DUST_CAPACITY = 384;
interface Puff {
  x: number;
  y: number;
  z: number;
  vx: number;
  vz: number;
  life: number;
  max: number;
  size: number;
}

/** Pooled, cosmetic dust: two triangles per puff, no textures, lights or physics bodies. */
export class TrackDust {
  readonly mesh: THREE.InstancedMesh;
  private opacity = new THREE.InstancedBufferAttribute(new Float32Array(TRACK_DUST_CAPACITY), 1);
  private color = { value: new THREE.Color(0xc3ad85) };
  private puffs: Puff[] = [];
  private free: Puff[] = Array.from({ length: TRACK_DUST_CAPACITY }, () => ({
    x: 0,
    y: 0,
    z: 0,
    vx: 0,
    vz: 0,
    life: 0,
    max: 0,
    size: 0,
  }));
  private poses = new Map<number, { x: number; z: number; pending: number }>();
  private previousTime?: number;
  private dummy = new THREE.Object3D();

  constructor() {
    const geometry = new THREE.PlaneGeometry(1, 1);
    geometry.setAttribute("puffOpacity", this.opacity);
    const material = new THREE.ShaderMaterial({
      uniforms: { dustColor: this.color },
      transparent: true,
      depthWrite: false,
      vertexShader: `
        attribute float puffOpacity;
        varying vec2 dustUv;
        varying float dustOpacity;
        void main() {
          dustUv = uv;
          dustOpacity = puffOpacity;
          vec4 center = modelViewMatrix * instanceMatrix * vec4(0.0, 0.0, 0.0, 1.0);
          center.xy += position.xy * vec2(length(instanceMatrix[0].xyz), length(instanceMatrix[1].xyz));
          gl_Position = projectionMatrix * center;
        }`,
      fragmentShader: `
        uniform vec3 dustColor;
        varying vec2 dustUv;
        varying float dustOpacity;
        void main() {
          float radius = length(dustUv * 2.0 - 1.0);
          float alpha = (1.0 - smoothstep(0.1, 1.0, radius)) * dustOpacity;
          gl_FragColor = vec4(dustColor, alpha);
          #include <tonemapping_fragment>
          #include <colorspace_fragment>
        }`,
    });
    this.mesh = new THREE.InstancedMesh(geometry, material, TRACK_DUST_CAPACITY);
    this.mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.opacity.setUsage(THREE.DynamicDrawUsage);
    this.mesh.count = 0;
    this.mesh.frustumCulled = false;
  }

  reset(): void {
    this.free.push(...this.puffs);
    this.puffs.length = 0;
    this.poses.clear();
    this.previousTime = undefined;
    this.mesh.count = 0;
  }

  update(simulation: Simulation): void {
    const elapsed = simulation.elapsed - (this.previousTime ?? simulation.elapsed);
    this.previousTime = simulation.elapsed;
    if (elapsed < 0) {
      this.reset();
      return;
    }
    if (elapsed === 0 || simulation.match.phase !== "playing") {
      return;
    }
    // A suspended tab or respawn must not generate a long catch-up trail.
    const dt = Math.min(elapsed, 0.1);
    const quarry = simulation.mapTheme === "quarry";
    const harbor = simulation.mapTheme === "harbor";
    const village = simulation.mapTheme === "village";
    this.color.value.setHex(quarry ? 0xd9bc8b : harbor ? 0xaeb0ab : 0xe1caa2);
    let live = 0;
    for (const puff of this.puffs) {
      puff.life -= elapsed;
      if (puff.life <= 0) {
        this.free.push(puff);
        continue;
      }
      puff.x += puff.vx * dt;
      puff.z += puff.vz * dt;
      puff.y += dt * 0.3;
      this.puffs[live++] = puff;
    }
    this.puffs.length = live;
    for (const [id] of this.poses) {
      if (!simulation.tanks.some((tank) => tank.id === id && tank.alive)) {
        this.poses.delete(id);
      }
    }
    for (const tank of simulation.tanks) {
      if (!tank.alive) {
        continue;
      }
      const p = tank.body.translation();
      let previous = this.poses.get(tank.id);
      if (!previous) {
        previous = { x: p.x, z: p.z, pending: 0 };
        this.poses.set(tank.id, previous);
      }
      const distance = Math.hypot(p.x - previous.x, p.z - previous.z);
      const velocity = tank.body.linvel();
      const speed = Math.hypot(velocity.x, velocity.z);
      const scale = VEHICLES[tank.kind].scale;
      const spacing = (quarry ? 1 : harbor ? 3 : 2.1) * scale;
      if (elapsed > 0.1 || distance > 5 || p.y > 1.25 || speed < 1.5) {
        previous.pending = 0;
      } else if (distance > 1e-6) {
        const sin = Math.sin(tank.heading);
        const cos = Math.cos(tank.heading);
        const direction = velocity.x * sin + velocity.z * cos >= 0 ? 1 : -1;
        const halfLength = (tank.kind === "scout" ? 2.1 : 2.6) * scale;
        for (let d = spacing - previous.pending; d <= distance; d += spacing) {
          const u = d / distance;
          for (const side of [-1, 1]) {
            const x =
              previous.x +
              (p.x - previous.x) * u -
              sin * halfLength * direction +
              cos * side * scale;
            const z =
              previous.z +
              (p.z - previous.z) * u -
              cos * halfLength * direction -
              sin * side * scale;
            if (village && !isVillageDirt(x, z)) {
              continue;
            }
            const puff = this.free.pop();
            if (!puff) {
              break;
            }
            puff.x = x;
            puff.z = z;
            puff.y = 0.25;
            puff.vx = -sin * direction * 0.4 + cos * side * 0.45;
            puff.vz = -cos * direction * 0.4 - sin * side * 0.45;
            puff.life = puff.max = (quarry ? 0.55 : 0.45) + Math.random() * 0.2;
            puff.size = (0.8 + Math.random() * 0.2) * scale * (quarry ? 1.15 : 1);
            this.puffs.push(puff);
          }
        }
        previous.pending = (previous.pending + distance) % spacing;
      }
      previous.x = p.x;
      previous.z = p.z;
    }
    this.mesh.count = this.puffs.length;
    for (let i = 0; i < this.puffs.length; i++) {
      const puff = this.puffs[i];
      const age = 1 - puff.life / puff.max;
      const size = puff.size * (0.7 + age * 1.8);
      this.dummy.position.set(puff.x, puff.y, puff.z);
      this.dummy.scale.set(size, size * 0.65, 1);
      this.dummy.updateMatrix();
      this.mesh.setMatrixAt(i, this.dummy.matrix);
      this.opacity.setX(
        i,
        Math.min(1, age * 12) * (1 - age) * (quarry ? 0.46 : harbor ? 0.12 : 0.38),
      );
    }
    updateInstances(this.mesh);
    if (this.mesh.count) {
      this.opacity.clearUpdateRanges();
      this.opacity.addUpdateRange(0, this.mesh.count);
      this.opacity.needsUpdate = true;
    }
  }
}
