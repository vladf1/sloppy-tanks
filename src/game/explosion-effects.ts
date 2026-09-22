import * as THREE from "three/webgpu";
import { attribute, uv, vec3, smoothstep, atan, sin } from "three/tsl";
import { billboardVertex } from "./effect-materials";
import { updateInstances, storageInstances } from "./render-resources";
import type { SimEvent } from "./types";

export const MAX_EXPLOSIONS = 24;
export const EXPLOSION_LIFETIME = 1.15;
export const TANK_EXPLOSION_LIFETIME = 2.1;
const PUFFS_PER_BLAST = 8;
const hot = new THREE.Color().setRGB(2.4, 1.25, 0.2);
const flame = new THREE.Color(0xf87924);
const smoke = new THREE.Color(0x62666a);
const tankSmoke = new THREE.Color(0x34373a);
const dust = new THREE.Color(0xaa9273);
// Distinct silhouettes and timing, using the same eight slots per tank death.
const TANK_BLASTS = [
  {
    lifetime: 2.1,
    stagger: 0.075,
    spread: 0.32,
    step: 0.58,
    rise: 1.65,
    drift: 0.3,
    smokeSize: 0.9,
    smoke: tankSmoke,
    fireEnd: 0.61,
    fireSpread: 0.24,
    fireStep: 0.65,
    fireRise: 2.2,
    fireSize: 0.64,
    fireStretch: 1.65,
    doubleBurst: true,
  },
  {
    lifetime: 1.65,
    stagger: 0.025,
    spread: 1.65,
    step: 0.08,
    rise: 0.85,
    drift: 0.2,
    smokeSize: 1.12,
    smoke: new THREE.Color(0x635448),
    fireEnd: 0.45,
    fireSpread: 0.85,
    fireStep: 0.16,
    fireRise: 0.7,
    fireSize: 1.15,
    fireStretch: 0.78,
    doubleBurst: false,
  },
  {
    lifetime: 1.95,
    stagger: 0.055,
    spread: 0.55,
    step: 0.26,
    rise: 1.05,
    drift: 1.8,
    smokeSize: 0.8,
    smoke: new THREE.Color(0x41474d),
    fireEnd: 0.52,
    fireSpread: 0.45,
    fireStep: 0.3,
    fireRise: 1.1,
    fireSize: 0.75,
    fireStretch: 1.2,
    doubleBurst: false,
  },
] as const;

const BARREL_BLASTS = [
  {
    lifetime: 1.15,
    stagger: 0.018,
    spread: 1.55,
    step: 0.05,
    rise: 0.8,
    drift: 0.15,
    smokeSize: 0.85,
    smoke: new THREE.Color(0x716357),
    fireEnd: 0.38,
    fireSpread: 0.9,
    fireStep: 0.1,
    fireRise: 0.7,
    fireSize: 1,
    fireStretch: 0.68,
    doubleBurst: false,
  },
  {
    lifetime: 1.35,
    stagger: 0.04,
    spread: 0.6,
    step: 0.18,
    rise: 1.1,
    drift: 2,
    smokeSize: 0.7,
    smoke: new THREE.Color(0x575b61),
    fireEnd: 0.48,
    fireSpread: 0.35,
    fireStep: 0.25,
    fireRise: 1,
    fireSize: 0.7,
    fireStretch: 1.15,
    doubleBurst: false,
  },
  {
    lifetime: 1.2,
    stagger: 0.035,
    spread: 1,
    step: 0.12,
    rise: 1.25,
    drift: 0.55,
    smokeSize: 0.75,
    smoke: new THREE.Color(0x66615b),
    fireEnd: 0.61,
    fireSpread: 0.5,
    fireStep: 0.23,
    fireRise: 1.25,
    fireSize: 0.65,
    fireStretch: 0.95,
    doubleBurst: true,
  },
] as const;

interface Blast {
  active: boolean;
  x: number;
  z: number;
  age: number;
  scale: number;
  phase: number;
  fire: boolean;
  tank: boolean;
  barrel: boolean;
  burnout: boolean;
  variant: number;
  tempo: number;
}

/** Analytic, pooled cosmetic blasts. Two draws, no textures, lights or physics bodies. */
export class ExplosionEffects {
  readonly group = new THREE.Group();
  private blasts: Blast[] = Array.from({ length: MAX_EXPLOSIONS }, () => ({
    active: false,
    x: 0,
    z: 0,
    age: 0,
    scale: 1,
    phase: 0,
    fire: true,
    tank: false,
    barrel: false,
    burnout: false,
    variant: 0,
    tempo: 1,
  }));
  private cursor = 0;
  private tankSequence = 0;
  private barrelSequence = 0;
  private dummy = new THREE.Object3D();
  private color = new THREE.Color();
  private puffColor = new THREE.InstancedBufferAttribute(
    new Float32Array(MAX_EXPLOSIONS * PUFFS_PER_BLAST * 4),
    4,
  );
  private ringAge = new THREE.InstancedBufferAttribute(new Float32Array(MAX_EXPLOSIONS * 2), 2);
  readonly puffs: THREE.InstancedMesh;
  readonly rings: THREE.InstancedMesh;

  constructor() {
    const geometry = new THREE.PlaneGeometry(2, 2);
    geometry.setAttribute("puffColor", this.puffColor);
    const point = uv().mul(2).sub(1);
    const radius = point.length();
    const tint = attribute("puffColor", "vec4" as const);
    const puffMaterial = new THREE.MeshBasicNodeMaterial({
      transparent: true,
      depthWrite: false,
      fog: false,
    });
    puffMaterial.colorNode = tint.rgb.mul(
      radius.oneMinus().add(point.y.mul(0.3)).clamp().mul(0.24).add(0.76),
    );
    puffMaterial.opacityNode = tint.a.mul(smoothstep(0.35, 1, radius).oneMinus());
    this.puffs = new THREE.InstancedMesh(geometry, puffMaterial, MAX_EXPLOSIONS * PUFFS_PER_BLAST);
    puffMaterial.vertexNode = billboardVertex(this.puffs);
    const plane = new THREE.PlaneGeometry(1, 1).rotateX(-Math.PI / 2);
    plane.setAttribute("ringAge", this.ringAge);
    const age = attribute("ringAge", "vec2" as const);
    const angle = atan(point.y, point.x);
    const breakup = sin(angle.mul(7).add(age.y))
      .mul(0.12)
      .add(sin(angle.mul(13).sub(age.y)).mul(0.08))
      .add(0.8);
    const band = smoothstep(0.43, 0.65, radius).mul(smoothstep(0.72, 0.98, radius).oneMinus());
    const fade = smoothstep(0, 0.07, age.x).mul(smoothstep(0.12, 0.55, age.x).oneMinus());
    const ringMaterial = new THREE.MeshBasicNodeMaterial({
      transparent: true,
      depthWrite: false,
      fog: false,
      polygonOffset: true,
      polygonOffsetFactor: -1,
      polygonOffsetUnits: -1,
    });
    ringMaterial.colorNode = vec3(0.48, 0.35, 0.21);
    ringMaterial.opacityNode = band.mul(breakup).mul(fade).mul(0.34);
    this.rings = new THREE.InstancedMesh(plane, ringMaterial, MAX_EXPLOSIONS);
    for (const mesh of [this.puffs, this.rings]) {
      storageInstances(mesh);
      mesh.frustumCulled = false;
      mesh.count = 0;
    }

    this.group.name = "expressive-explosions";
    this.group.add(this.rings, this.puffs);
  }

  event(event: SimEvent): void {
    const fire = event.type === "explosion" || event.type === "death";
    // Barrels also emit an explosion; do not double their fireball/dust budget.
    if (
      !fire &&
      (event.type !== "destroy" || event.coverKind === "tree" || event.coverKind === "drum")
    ) {
      return;
    }
    const blast = this.blasts[this.cursor];
    this.cursor = (this.cursor + 1) % MAX_EXPLOSIONS;
    blast.active = true;
    blast.x = event.x;
    blast.z = event.z;
    blast.age = 0;
    blast.tank = event.type === "death";
    blast.burnout = blast.tank && event.deathStyle === "burnout";
    blast.barrel = event.type === "explosion" && event.coverKind === "drum";
    blast.variant = blast.tank
      ? this.tankSequence++ % TANK_BLASTS.length
      : blast.barrel
        ? this.barrelSequence++ % BARREL_BLASTS.length
        : 0;
    blast.tempo = blast.tank || blast.barrel ? 0.9 + Math.random() * 0.2 : 1;
    blast.scale = blast.tank
      ? ((event.size ?? 3) / 3) * (0.9 + Math.random() * 0.25)
      : THREE.MathUtils.clamp((event.size ?? 3) / 4.5, 0.55, 1.45);
    if (blast.barrel) {
      blast.scale *= 0.88 + Math.random() * 0.25;
    }
    blast.phase = Math.random() * Math.PI * 2;
    blast.fire = fire;
  }

  reset(): void {
    for (const blast of this.blasts) {
      blast.active = false;
    }
    this.cursor = 0;
    this.tankSequence = 0;
    this.barrelSequence = 0;
    this.puffs.count = this.rings.count = 0;
  }

  private puff(
    x: number,
    y: number,
    z: number,
    size: number,
    color: THREE.Color,
    opacity: number,
    phase: number,
    height = 0.86,
  ): void {
    if (size <= 0 || opacity <= 0) {
      return;
    }
    const i = this.puffs.count++;
    this.dummy.position.set(x, y, z);
    this.dummy.rotation.set(phase, phase * 0.7, phase * 0.3);
    this.dummy.scale.set(size, size * height, size);
    this.dummy.updateMatrix();
    this.puffs.setMatrixAt(i, this.dummy.matrix);
    this.puffColor.setXYZW(i, color.r, color.g, color.b, opacity);
  }

  update(dt: number): void {
    this.puffs.count = this.rings.count = 0;
    for (const b of this.blasts) {
      if (!b.active) {
        continue;
      }
      b.age += Math.max(0, dt);
      const profile = b.tank
        ? TANK_BLASTS[b.variant]
        : b.barrel
          ? BARREL_BLASTS[b.variant]
          : undefined;
      const lifetime = b.burnout ? 2.7 : (profile?.lifetime ?? EXPLOSION_LIFETIME);
      const t = b.age * b.tempo;
      if (t >= lifetime) {
        b.active = false;
        continue;
      }
      const s = b.scale;
      if (b.burnout) {
        // A small internal flash, then thin exhaust-like smoke; no shockwave.
        for (let j = 0; j < 3; j++) {
          const age = t - j * 0.18;
          if (age <= 0) {
            continue;
          }
          const alpha = Math.min(1, age * 7) * (1 - THREE.MathUtils.smoothstep(t, 1.5, 2.7)) * 0.6;
          this.puff(
            b.x + Math.cos(b.phase) * age * 0.35,
            (1.1 + j * 0.3 + age * 0.75) * s,
            b.z + Math.sin(b.phase) * age * 0.35,
            (0.22 + Math.sqrt(age) * 0.3) * s,
            tankSmoke,
            alpha,
            b.phase,
          );
        }
        if (t < 0.12) {
          this.puff(b.x, 0.8 * s, b.z, 0.32 * s, flame, 1 - t / 0.12, b.phase);
        }
        continue;
      }
      // A column, a low rolling cloud, or a drifting side plume.
      // All profiles reuse the same five smoke and three fire slots.
      for (let j = 0; j < 5; j++) {
        const age = t - (b.fire ? 0.08 : 0) - j * (profile?.stagger ?? 0.018);
        if (age <= 0) {
          continue;
        }
        const angle = b.phase + (j * Math.PI * 2) / 5;
        const spread = (profile ? 0.18 + age * profile.spread : 0.35 + age * 1.15) * s;
        const alpha =
          Math.min(1, age / 0.1) *
          (1 - THREE.MathUtils.smoothstep(t, b.tank ? 1.05 : 0.45, lifetime)) *
          0.8;
        this.color.copy(profile?.smoke ?? (b.fire ? smoke : dust)).multiplyScalar(0.86 + j * 0.055);
        this.puff(
          b.x +
            Math.cos(angle) * spread +
            (profile ? Math.cos(b.phase) * t * profile.drift : t * 0.35),
          (profile
            ? (b.tank ? 0.9 : 0.55) + j * profile.step + age * profile.rise
            : 0.65 + age * 1.45 + (j % 2) * 0.25) * s,
          b.z + Math.sin(angle) * spread + (profile ? Math.sin(b.phase) * t * profile.drift : 0),
          (0.45 + Math.sqrt(age) * (profile?.smokeSize ?? 0.9)) * s,
          this.color,
          alpha,
          angle,
        );
      }
      const fireEnd = profile?.fireEnd ?? 0.38;
      const fireAge = profile?.doubleBurst && t >= 0.34 ? t - 0.34 : t;
      const pulseLength = profile?.doubleBurst ? 0.27 : fireEnd;
      if (b.fire && t < fireEnd && fireAge < pulseLength) {
        const growth = Math.min(1, fireAge / 0.055);
        const fade =
          1 - THREE.MathUtils.smoothstep(fireAge, profile ? pulseLength * 0.47 : 0.18, pulseLength);
        for (let j = 0; j < 3; j++) {
          const angle = b.phase + (j * Math.PI * 2) / 3;
          this.color.copy(hot).lerp(flame, Math.min(1, fireAge * (j === 0 ? 2 : 5) + j * 0.3));
          const drift = profile ? profile.drift * fireAge * 0.7 : 0;
          this.puff(
            b.x + Math.cos(angle) * (profile?.fireSpread ?? 0.55) * s + Math.cos(b.phase) * drift,
            (profile
              ? (b.tank ? 1 : 0.65) + j * profile.fireStep + fireAge * profile.fireRise
              : 0.8 + j * 0.25 + t * 1.2) * s,
            b.z + Math.sin(angle) * (profile?.fireSpread ?? 0.55) * s + Math.sin(b.phase) * drift,
            (profile ? profile.fireSize + j * 0.07 + fireAge * 0.3 : 0.85 + j * 0.12 + t * 0.7) *
              growth *
              s,
            this.color,
            fade,
            angle,
            profile?.fireStretch ?? 0.86,
          );
        }
      }
      if (t < 0.55) {
        const i = this.rings.count++;
        const radius = (0.65 + 3.6 * (1 - Math.exp(-t * 5))) * s;
        this.dummy.position.set(b.x, 0.07, b.z);
        this.dummy.rotation.set(0, 0, 0);
        this.dummy.scale.set(radius * 2, 1, radius * 2);
        this.dummy.updateMatrix();
        this.rings.setMatrixAt(i, this.dummy.matrix);
        this.ringAge.setXY(i, t, b.phase);
      }
    }
    for (const [mesh, attribute] of [
      [this.puffs, this.puffColor],
      [this.rings, this.ringAge],
    ] as const) {
      updateInstances(mesh);
      if (mesh.count) {
        attribute.clearUpdateRanges();
        attribute.addUpdateRange(0, mesh.count * attribute.itemSize);
        attribute.needsUpdate = true;
      }
    }
  }
}
