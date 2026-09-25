import * as THREE from "three/webgpu";
import { uniform } from "three/tsl";
import { DUST_OPACITY, dustMaterial, billboardVertex } from "./effect-materials";
import { updateInstances, storageInstances } from "./render-resources";
import type { Simulation } from "./simulation";
import { renderState, type RenderState } from "./render-state";

export const QUARRY_DUST_CAPACITY = 48;
export const QUARRY_DUST_MAX_OPACITY = 0.1;

interface Wisp {
  x: number;
  y: number;
  z: number;
  vx: number;
  life: number;
  max: number;
  size: number;
  phase: number;
  alpha: number;
}

/** Sparse windblown wisps along the quarry apron, plus rare faint sheets high
 * over the combat floor: one bounded instanced draw, no textures, lights,
 * physics bodies or per-frame allocations. Cosmetic randomness is deliberately
 * independent from the seeded simulation. */
export class QuarryDust {
  readonly mesh: THREE.InstancedMesh;
  private opacity = new THREE.InstancedBufferAttribute(new Float32Array(QUARRY_DUST_CAPACITY), 1);
  private wisps: Wisp[] = [];
  private free: Wisp[] = Array.from({ length: QUARRY_DUST_CAPACITY }, () => ({
    x: 0,
    y: 0,
    z: 0,
    vx: 0,
    life: 0,
    max: 0,
    size: 0,
    phase: 0,
    alpha: 1,
  }));
  private timer = 0;
  private dummy = new THREE.Object3D();

  constructor() {
    const geometry = new THREE.PlaneGeometry(1, 1);
    geometry.setAttribute(DUST_OPACITY, this.opacity);
    const material = dustMaterial(uniform(new THREE.Color(0xe3cfa5)));
    this.mesh = new THREE.InstancedMesh(geometry, material, QUARRY_DUST_CAPACITY);
    material.vertexNode = billboardVertex(this.mesh);
    this.mesh.name = "quarry-wind-dust";
    storageInstances(this.mesh);

    this.mesh.count = 0;
    this.mesh.frustumCulled = false;
    this.mesh.visible = false;
  }

  reset(): void {
    this.free.push(...this.wisps);
    this.wisps.length = 0;
    this.timer = 0;
    this.mesh.count = 0;
    this.mesh.visible = false;
  }

  private spawn(): void {
    const wisp = this.free.pop();
    if (!wisp) {
      return;
    }
    // One spawn in five drifts high over the combat floor instead: larger,
    // fainter and well above the tanks so readability never suffers.
    if (Math.random() < 0.2) {
      wisp.x = -50 + Math.random() * 100;
      wisp.z = -50 + Math.random() * 100;
      wisp.y = 2.4 + Math.random() * 2.4;
      wisp.vx = 1.5 + Math.random() * 1.5;
      wisp.life = wisp.max = 6 + Math.random() * 3;
      wisp.size = 5 + Math.random() * 3.5;
      wisp.phase = Math.random() * Math.PI * 2;
      wisp.alpha = 0.45;
      this.wisps.push(wisp);
      return;
    }
    const side = Math.random() < 0.5 ? -1 : 1;
    const z = side * (63 + Math.random() * 8);
    // Rest on the dipped apron outside the wall, the same grade the terrain bakes.
    const ground = -Math.min(1.8, (Math.abs(z) - 60) * 0.3);
    wisp.x = -70 + Math.random() * 140;
    wisp.z = z;
    wisp.y = ground + 0.5 + Math.random() * 0.9;
    wisp.vx = 1.2 + Math.random() * 1.2;
    wisp.life = wisp.max = 5 + Math.random() * 3;
    wisp.size = 2.5 + Math.random() * 2;
    wisp.phase = Math.random() * Math.PI * 2;
    wisp.alpha = 1;
    this.wisps.push(wisp);
  }

  update(source: Simulation | RenderState, dt: number): void {
    const simulation = renderState(source);
    if (simulation.mapTheme !== "quarry") {
      if (this.mesh.count > 0 || this.mesh.visible) {
        this.reset();
      }
      return;
    }
    this.mesh.visible = true;
    // Frozen while paused or between rounds, like the track dust pool.
    if (simulation.match.phase !== "playing") {
      return;
    }
    const step = Math.min(dt, 0.1);
    this.timer -= step;
    if (this.timer <= 0) {
      this.timer = 0.35 + Math.random() * 0.6;
      this.spawn();
    }
    let live = 0;
    for (const wisp of this.wisps) {
      wisp.life -= step;
      if (wisp.life <= 0) {
        this.free.push(wisp);
        continue;
      }
      wisp.x += wisp.vx * step;
      wisp.z += Math.sin(simulation.elapsed * 0.6 + wisp.phase) * 0.5 * step;
      this.wisps[live++] = wisp;
    }
    this.wisps.length = live;
    this.mesh.count = live;
    for (let i = 0; i < live; i++) {
      const wisp = this.wisps[i];
      const age = 1 - wisp.life / wisp.max;
      const size = wisp.size * (0.8 + age * 1.2);
      this.dummy.position.set(wisp.x, wisp.y, wisp.z);
      this.dummy.scale.set(size, size * 0.55, 1);
      this.dummy.updateMatrix();
      this.mesh.setMatrixAt(i, this.dummy.matrix);
      this.opacity.setX(i, Math.sin(age * Math.PI) * QUARRY_DUST_MAX_OPACITY * wisp.alpha);
    }
    updateInstances(this.mesh);
    if (this.mesh.count) {
      this.opacity.clearUpdateRanges();
      this.opacity.addUpdateRange(0, this.mesh.count);
      this.opacity.needsUpdate = true;
    }
  }
}
