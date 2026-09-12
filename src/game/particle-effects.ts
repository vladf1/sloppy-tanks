import * as THREE from "three";
import { updateInstances } from "./render-resources";
import type { SimEvent } from "./types";

const MAX_PARTICLES = 1200;
const PARTICLE_GRAVITY = 8;
export interface Particle {
  shape?: "leaf" | "splinter";
  x: number;
  y: number;
  z: number;
  vx: number;
  vy: number;
  vz: number;
  life: number;
  max: number;
  size: number;
  color: THREE.Color;
}
// Life and size pairs are [minimum, random span]. Choose once per event.
const PARTICLE_STYLES = {
  treeHit: {
    count: 18,
    life: [0.45, 0.35],
    size: [0.1, 0.12],
    speed: 3.8,
    scatter: 0.55,
    height: 0.7,
    lift: 1.8,
  },
  woodHit: {
    count: 12,
    life: [0.45, 0.4],
    size: [0.07, 0.1],
    speed: 4,
    scatter: 0.35,
    height: 0.8,
    lift: 1.5,
  },
  tree: {
    count: 96,
    life: [0.85, 0.9],
    size: [0.18, 0.25],
    speed: 7,
    scatter: 1.5,
    height: 0.6,
    lift: 1,
  },
  pickup: {
    count: 24,
    life: [0.5, 0.3],
    size: [0.12, 0.1],
    speed: 5,
    scatter: 0,
    height: 1.3,
    lift: 4,
  },
  explosion: {
    count: 18,
    life: [0.35, 0.45],
    size: [0.22, 0.5],
    speed: 1.2,
    scatter: 0,
    height: 0.8,
    lift: 0,
  },
  hurt: {
    count: 12,
    life: [0.22, 0.16],
    size: [0.09, 0.09],
    speed: 6,
    scatter: 0.9,
    height: 2.1,
    lift: 1.5,
  },
  impact: {
    count: 8,
    life: [0.1, 0.2],
    size: [0.04, 0.09],
    speed: 4,
    scatter: 0,
    height: 1,
    lift: 0,
  },
};
/** Cosmetic randomness is deliberately independent from the seeded simulation. */
export class ParticleEffects {
  readonly particles: Particle[] = [];
  readonly mesh: THREE.InstancedMesh;
  private dummy = new THREE.Object3D();
  constructor() {
    this.mesh = new THREE.InstancedMesh(
      new THREE.IcosahedronGeometry(1, 0),
      new THREE.MeshBasicMaterial({ color: 0xffffff }),
      MAX_PARTICLES,
    );
    this.mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.mesh.frustumCulled = false;
  }
  reset(): void {
    this.particles.length = 0;
    this.mesh.count = 0;
  }
  event(event: SimEvent): boolean {
    const pickup = event.type === "pickup" || event.type === "promotion";
    const hurt = event.type === "hurt";
    const explosion =
      event.type === "explosion" || event.type === "death" || event.type === "destroy";
    const coverEffect = event.type === "destroy" || event.type === "impact";
    const timber =
      coverEffect &&
      (event.coverKind === "timber" || event.coverKind === "fence" || event.coverKind === "cargo");
    const tree = coverEffect && event.coverKind === "tree";
    const chipHit = event.type === "impact" && (tree || timber);
    const style =
      PARTICLE_STYLES[
        chipHit
          ? tree
            ? "treeHit"
            : "woodHit"
          : tree
            ? "tree"
            : pickup
              ? "pickup"
              : explosion
                ? "explosion"
                : hurt
                  ? "hurt"
                  : "impact"
      ];
    const count = event.type === "shot" ? 5 : style.count;
    const baseSpeed = style.speed * (explosion && !tree ? (event.size ?? 3) : 1);
    const colors = timber
      ? [0x805336, 0xb47a49, 0xc99a65, 0x947958]
      : tree
        ? Array.from({ length: 12 }, (_, i) =>
            i % 4 === 0 ? 0x98633e : [0x175e3b, 0x2c9452, event.color ?? 0x389b58][i % 3],
          )
        : pickup
          ? [0xffffff, event.color ?? 0xffffff, event.color ?? 0xffffff, event.color ?? 0xffffff]
          : explosion
            ? [0x536779, 0xff9250, 0xffc569, 0x536779, 0xffc569, 0xff9250]
            : hurt
              ? [0xffffff, 0xffcb58, 0xffcb58]
              : [event.color ?? 0xffdf91];
    for (let i = 0; i < count && this.particles.length < MAX_PARTICLES; i++) {
      const life =
        (style.life[0] + Math.random() * style.life[1]) *
        (tree ? 3 : timber || (event.type === "destroy" && event.coverKind === "fence") ? 2 : 1);
      const speed = baseSpeed + (tree && !chipHit ? Math.random() * 4 : 0);
      this.particles.push({
        shape: timber ? "splinter" : tree ? (i % 4 === 0 ? "splinter" : "leaf") : undefined,
        x: event.x + (Math.random() - 0.5) * style.scatter,
        y:
          chipHit && tree
            ? i % 4 === 0
              ? 0.7 + Math.random() * 0.4
              : (event.height ?? 5) * (0.45 + Math.random() * 0.35)
            : style.height + (tree ? Math.random() * (event.height ?? 5) * 0.85 : 0),
        z: event.z + (Math.random() - 0.5) * style.scatter,
        vx: (Math.random() - 0.5) * speed,
        vy: style.lift + Math.random() * (tree ? 5 : speed),
        vz: (Math.random() - 0.5) * speed,
        life,
        max: life,
        size: style.size[0] + Math.random() * style.size[1],
        color: new THREE.Color(colors[i % colors.length]),
      });
    }
    return explosion && !tree;
  }
  update(dt: number, time: number): void {
    let live = 0;
    for (const q of this.particles) {
      q.life -= dt;
      if (q.life <= 0) {
        continue;
      }
      q.x += q.vx * dt;
      q.y += q.vy * dt;
      q.z += q.vz * dt;
      q.vy -= PARTICLE_GRAVITY * dt;
      this.particles[live++] = q;
    }
    this.particles.length = live;
    this.mesh.count = this.particles.length;
    for (let i = 0; i < this.particles.length; i++) {
      const q = this.particles[i];
      this.dummy.position.set(q.x, Math.max(0.1, q.y), q.z);
      this.dummy.rotation.set(0, time, 0);
      this.dummy.scale.setScalar((q.size * q.life) / q.max);
      if (q.shape) {
        this.dummy.rotation.set(time * 5 + i, time * 3 + i, time * 4);
        this.dummy.scale.x *= q.shape === "leaf" ? 1.5 : 0.4;
        this.dummy.scale.y *= q.shape === "leaf" ? 0.25 : 2.4;
        this.dummy.scale.z *= q.shape === "leaf" ? 0.8 : 0.4;
      }
      this.dummy.updateMatrix();
      this.mesh.setMatrixAt(i, this.dummy.matrix);
      this.mesh.setColorAt(i, q.color);
    }
    updateInstances(this.mesh);
  }
}
