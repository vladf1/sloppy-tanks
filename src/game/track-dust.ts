import * as THREE from "three/webgpu";
import { angleDelta, VEHICLES } from "./data";
import { uniform } from "three/tsl";
import { dustMaterial, billboardVertex } from "./effect-materials";
import { updateInstances, storageInstances } from "./render-resources";
import type { Simulation } from "./simulation";
import { renderState, type RenderState } from "./render-state";
import { TrackGravel } from "./track-gravel";
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
  readonly gravel = new TrackGravel();
  private opacity = new THREE.InstancedBufferAttribute(new Float32Array(TRACK_DUST_CAPACITY), 1);
  private color = uniform(new THREE.Color(0xc3ad85));
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
  private poses = new Map<
    number,
    { x: number; z: number; heading: number; pending: number; gravelCooldown: number }
  >();
  private previousTime?: number;
  private dummy = new THREE.Object3D();

  constructor() {
    const geometry = new THREE.PlaneGeometry(1, 1);
    geometry.setAttribute("puffOpacity", this.opacity);
    const material = dustMaterial("puffOpacity", this.color);
    this.mesh = new THREE.InstancedMesh(geometry, material, TRACK_DUST_CAPACITY);
    material.vertexNode = billboardVertex(this.mesh);
    storageInstances(this.mesh);

    this.mesh.count = 0;
    this.mesh.frustumCulled = false;
  }

  reset(): void {
    this.gravel.reset();
    this.free.push(...this.puffs);
    this.puffs.length = 0;
    this.poses.clear();
    this.previousTime = undefined;
    this.mesh.count = 0;
  }

  update(source: Simulation | RenderState): void {
    const simulation = renderState(source);
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
    this.gravel.update(elapsed);
    const quarry = simulation.mapTheme === "quarry";
    const harbor = simulation.mapTheme === "harbor";
    const village = simulation.mapTheme === "village";
    const grassFloor = simulation.mapFloor === "dry-grass";
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
      const p = tank.position;
      let previous = this.poses.get(tank.id);
      if (!previous) {
        previous = { x: p.x, z: p.z, heading: tank.heading, pending: 0, gravelCooldown: 0 };
        this.poses.set(tank.id, previous);
      }
      const distance = Math.hypot(p.x - previous.x, p.z - previous.z);
      const velocity = tank.velocity;
      const speed = Math.hypot(velocity.x, velocity.z);
      const scale = VEHICLES[tank.kind].scale;
      const turn = angleDelta(previous.heading, tank.heading);
      const sin = Math.sin(tank.heading);
      const cos = Math.cos(tank.heading);
      const forward = velocity.x * sin + velocity.z * cos;
      const lateral = Math.abs(velocity.x * cos - velocity.z * sin);
      const strength = THREE.MathUtils.clamp(
        Math.abs(turn) / dt / 2.4 + (lateral / (speed + 1)) * 0.6,
        0,
        1,
      );
      const halfLength = (tank.kind === "scout" ? 2.1 : 2.6) * scale;
      // Use the faster of translation and belt travel during a pivot, not both added.
      // Turns stir the same surface as driving; they do not multiply dust production.
      const contactTravel = Math.max(distance, Math.abs(turn) * scale);
      const spacing = (quarry ? 1 : harbor ? 3 : 2.1) * scale;
      previous.gravelCooldown -= dt;
      if (
        elapsed > 0.1 ||
        distance > 5 ||
        Math.abs(turn) > 0.8 ||
        p.y > 1.25 ||
        contactTravel / dt < 1.5
      ) {
        previous.pending = 0;
      } else if (contactTravel > 1e-6) {
        let threwGravel = false;
        for (let d = spacing - previous.pending; d <= contactTravel; d += spacing) {
          const u = THREE.MathUtils.clamp(d / contactTravel, 0, 1);
          const heading = previous.heading + turn * u;
          const sin = Math.sin(heading);
          const cos = Math.cos(heading);
          for (const side of [-1, 1]) {
            const trackSpeed = forward - ((side * turn) / dt) * scale;
            const direction = trackSpeed >= 0 ? 1 : -1;
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
            if (grassFloor || (village && !isVillageDirt(x, z))) {
              continue;
            }
            const vx = -sin * direction * 0.4 + cos * side * (0.45 + strength * 0.35);
            const vz = -cos * direction * 0.4 - sin * side * (0.45 + strength * 0.35);
            if (quarry && previous.gravelCooldown <= 0 && (strength > 0.25 || speed > 10)) {
              this.gravel.emit(x, z, vx * 1.5, vz * 1.5, strength);
              threwGravel = true;
            }
            const puff = this.free.pop();
            if (!puff) {
              break;
            }
            puff.x = x;
            puff.z = z;
            puff.y = 0.25;
            puff.vx = vx;
            puff.vz = vz;
            puff.life = puff.max = (quarry ? 0.55 : 0.45) + Math.random() * 0.2;
            puff.size =
              (0.8 + Math.random() * 0.2) * scale * (quarry ? 1.15 : 1) * (1 + strength * 0.1);
            this.puffs.push(puff);
          }
          if (threwGravel) {
            previous.gravelCooldown = 0.12;
          }
        }
        previous.pending = (previous.pending + contactTravel) % spacing;
      }
      previous.x = p.x;
      previous.z = p.z;
      previous.heading = tank.heading;
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
