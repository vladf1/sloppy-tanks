import * as THREE from "three";
import { updateInstances, storageInstances } from "./render-resources";

export const TRACK_GRAVEL_CAPACITY = 192;
interface Pebble {
  x: number;
  y: number;
  z: number;
  vx: number;
  vy: number;
  vz: number;
  life: number;
  size: number;
  spin: number;
  bounce: boolean;
}

/** Tiny cosmetic chips: one bounded draw, no collision bodies or scene queries. */
export class TrackGravel {
  readonly mesh = new THREE.InstancedMesh(
    new THREE.TetrahedronGeometry(1, 0),
    new THREE.MeshStandardMaterial({ color: 0xb09c7b, roughness: 1, flatShading: true }),
    TRACK_GRAVEL_CAPACITY,
  );
  private live: Pebble[] = [];
  private free: Pebble[] = Array.from({ length: TRACK_GRAVEL_CAPACITY }, () => ({
    x: 0,
    y: 0,
    z: 0,
    vx: 0,
    vy: 0,
    vz: 0,
    life: 0,
    size: 0,
    spin: 0,
    bounce: false,
  }));
  private dummy = new THREE.Object3D();

  constructor() {
    this.mesh.name = "quarry-track-gravel";
    storageInstances(this.mesh);
    this.mesh.frustumCulled = false;
    this.mesh.count = 0;
  }

  emit(x: number, z: number, vx: number, vz: number, strength: number): void {
    for (let i = 0; i < (strength > 0.45 ? 2 : 1); i++) {
      const p = this.free.pop();
      if (!p) {
        return;
      }
      p.x = x;
      p.y = 0.15;
      p.z = z;
      p.vx = vx + (Math.random() - 0.5) * 0.8;
      p.vz = vz + (Math.random() - 0.5) * 0.8;
      p.vy = 1.6 + Math.random() * 1.4 + strength * 0.6;
      p.life = 0.5 + Math.random() * 0.22;
      p.size = 0.055 + Math.random() * 0.05;
      p.spin = Math.random() * Math.PI * 2;
      p.bounce = false;
      this.live.push(p);
    }
  }

  reset(): void {
    this.free.push(...this.live);
    this.live.length = 0;
    this.mesh.count = 0;
  }

  update(elapsed: number): void {
    const dt = Math.min(elapsed, 0.1);
    let count = 0;
    for (const p of this.live) {
      p.life -= elapsed;
      if (p.life <= 0) {
        this.free.push(p);
        continue;
      }
      p.x += p.vx * dt;
      p.z += p.vz * dt;
      p.y += p.vy * dt;
      p.vy -= 12 * dt;
      p.spin += dt * 12;
      if (p.y < 0.055) {
        p.y = 0.055;
        p.vy = p.bounce ? 0 : Math.abs(p.vy) * 0.25;
        p.vx *= 0.6;
        p.vz *= 0.6;
        p.bounce = true;
      }
      this.live[count] = p;
      this.dummy.position.set(p.x, p.y, p.z);
      this.dummy.rotation.set(p.spin, p.spin * 0.7, 0);
      this.dummy.scale.setScalar(p.size * Math.min(1, p.life / 0.14));
      this.dummy.updateMatrix();
      this.mesh.setMatrixAt(count++, this.dummy.matrix);
    }
    this.live.length = count;
    this.mesh.count = count;
    updateInstances(this.mesh);
  }
}
