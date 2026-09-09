import * as THREE from "three";
import { PICKUPS } from "./data";
import { tankMuzzle } from "./hitboxes";
import type { Simulation } from "./simulation";
import type { SimEvent } from "./types";

const CAPACITY = 64;
const FLASH_SECONDS = 0.12;
export class LaserVisuals {
  group = new THREE.Group();
  private pose = new THREE.Object3D();
  private direction = new THREE.Vector3();
  private up = new THREE.Vector3(0, 1, 0);
  private beams: { from: THREE.Vector3; to: THREE.Vector3; life: number }[] = [];
  private layer(geometry: THREE.BufferGeometry, color: number, opacity = 1) {
    const mesh = new THREE.InstancedMesh(
      geometry,
      new THREE.MeshBasicMaterial({
        color,
        transparent: opacity < 1,
        opacity,
        depthWrite: opacity === 1,
        toneMapped: false,
      }),
      CAPACITY,
    );
    mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    mesh.frustumCulled = false;
    mesh.count = 0;
    this.group.add(mesh);
    return mesh;
  }
  halo = this.layer(new THREE.CylinderGeometry(0.045, 0.045, 1, 5), PICKUPS.laser.color, 0.5);
  core = this.layer(new THREE.CylinderGeometry(0.014, 0.014, 1, 5), 0xffffff);
  mount = this.layer(new THREE.CylinderGeometry(0.15, 0.15, 0.12, 8), 0x263b4c);
  lens = this.layer(new THREE.IcosahedronGeometry(0.105, 1), PICKUPS.laser.color);
  reset(): void {
    this.beams.length = 0;
    for (const mesh of this.group.children as THREE.InstancedMesh[]) {
      mesh.count = 0;
    }
  }
  event(event: SimEvent): void {
    if (event.type !== "laser" || !event.from) {
      return;
    }
    if (this.beams.length === CAPACITY) {
      this.beams.shift();
    }
    this.beams.push({
      from: new THREE.Vector3(event.from.x, event.from.y, event.from.z),
      to: new THREE.Vector3(event.x, event.height ?? 1, event.z),
      life: FLASH_SECONDS,
    });
  }
  update(simulation: Simulation, alpha: number, dt: number): void {
    const pose = this.pose;
    this.halo.count = this.core.count = this.mount.count = this.lens.count = 0;
    let live = 0;
    for (const beam of this.beams) {
      if (simulation.match.phase === "playing") {
        beam.life -= dt;
      }
      if (beam.life <= 0) {
        continue;
      }
      this.beams[live++] = beam;
      this.direction.subVectors(beam.to, beam.from);
      const length = this.direction.length();
      if (length < 1e-6) {
        continue;
      }
      pose.position.copy(beam.from).add(beam.to).multiplyScalar(0.5);
      pose.quaternion.setFromUnitVectors(this.up, this.direction.divideScalar(length));
      pose.scale.set(beam.life / FLASH_SECONDS, length, beam.life / FLASH_SECONDS);
      pose.updateMatrix();
      this.halo.setMatrixAt(this.halo.count++, pose.matrix);
      this.core.setMatrixAt(this.core.count++, pose.matrix);
    }
    this.beams.length = live;
    for (const tank of simulation.tanks) {
      if (!tank.alive || tank.laser <= 0 || this.lens.count === CAPACITY) {
        continue;
      }
      const position = tank.body.translation();
      pose.position.set(
        THREE.MathUtils.lerp(tank.previous.x, position.x, alpha),
        position.y - 0.4 + tankMuzzle(tank.kind).y + 0.3,
        THREE.MathUtils.lerp(tank.previous.z, position.z, alpha),
      );
      pose.quaternion.identity();
      pose.scale.setScalar(1);
      pose.updateMatrix();
      this.lens.setMatrixAt(this.lens.count++, pose.matrix);
      pose.position.y -= 0.075;
      pose.updateMatrix();
      this.mount.setMatrixAt(this.mount.count++, pose.matrix);
    }
    for (const mesh of this.group.children as THREE.InstancedMesh[]) {
      if (!mesh.count) {
        continue;
      }
      mesh.instanceMatrix.clearUpdateRanges();
      mesh.instanceMatrix.addUpdateRange(0, mesh.count * 16);
      mesh.instanceMatrix.needsUpdate = true;
    }
  }
}
