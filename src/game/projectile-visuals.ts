import * as THREE from "three";
import { mergeGeometries } from "three/addons/utils/BufferGeometryUtils.js";
import { AMMO_ORDER } from "./ammunition";
import { TEAM_COLORS, WEAPONS } from "./data";
import type { Shot, Weapon } from "./types";

const CAPACITY = 600;
// Preserve the original shell scale: rocket body is ~0.94 m, standard ~0.70 m.
const MODEL_SCALE: Record<Weapon, number> = { standard: 1, spread: 0.9, rocket: 0.65, ricochet: 0.65, piercing: 0.8 };
type Part = [THREE.BufferGeometry, number];
type Batch = { body: THREE.InstancedMesh; team: THREE.InstancedMesh; exhaust?: THREE.InstancedMesh };

// Author once along +Z, then instance the complete shapes in flight.
const tube = (radius: number, length: number, z = 0) =>
  new THREE.CylinderGeometry(radius, radius, length, 10).rotateX(Math.PI / 2).translate(0, 0, z);
const point = (radius: number, length: number, z: number) =>
  new THREE.ConeGeometry(radius, length, 10).rotateX(Math.PI / 2).translate(0, 0, z);
function painted(parts: Part[]) {
  const geometries = parts.map(([source, hex]) => {
    const geometry = source.index ? source.toNonIndexed() : source;
    if (source !== geometry) source.dispose();
    const color = new THREE.Color(hex);
    const colors = new Float32Array(geometry.getAttribute("position").count * 3);
    for (let i = 0; i < colors.length; i += 3) {
      colors[i] = color.r; colors[i + 1] = color.g; colors[i + 2] = color.b;
    }
    geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));
    return geometry;
  });
  const merged = mergeGeometries(geometries)!;
  for (const geometry of geometries) geometry.dispose();
  return merged;
}
function rocketFins() {
  const profile = new THREE.Shape();
  profile.moveTo(0.14, -0.12);
  profile.lineTo(0.4, -0.6);
  profile.lineTo(0.14, -0.48);
  profile.closePath();
  return new THREE.ExtrudeGeometry(profile, { depth: 0.055, bevelEnabled: false, steps: 1 })
    .translate(0, 0, -0.0275).rotateX(Math.PI / 2);
}
function rocketExhaust() {
  const geometry = painted([[point(0.16, 0.64, 0).rotateY(Math.PI).translate(0, 0, -0.82), 0xff671c]]);
  const positions = geometry.getAttribute("position"), colors = geometry.getAttribute("color");
  const hot = new THREE.Color(0xfff2b0), tip = new THREE.Color(0xff671c), color = new THREE.Color();
  for (let i = 0; i < positions.count; i++) {
    color.copy(hot).lerp(tip, THREE.MathUtils.clamp((-positions.getZ(i) - 0.5) / 0.64, 0, 1));
    colors.setXYZ(i, color.r, color.g, color.b);
  }
  return geometry;
}
function model(weapon: Weapon): { body: THREE.BufferGeometry; team: THREE.BufferGeometry; exhaust?: THREE.BufferGeometry } {
  const accent = WEAPONS[weapon].color, dark = 0x263344, steel = 0xe2e9ef;
  switch (weapon) {
    case "standard": return {
      body: painted([[tube(0.13, 0.38, -0.08), 0xcdaa55],
        [point(0.13, 0.26, 0.24), 0xffedb4], [tube(0.15, 0.08, -0.29), dark]]),
      team: tube(0.138, 0.27, -0.07),
    };
    case "spread": return {
      body: painted([[new THREE.IcosahedronGeometry(0.18, 1), dark],
        [new THREE.SphereGeometry(0.14, 8, 6).translate(0, 0.055, 0.035), accent]]),
      team: new THREE.SphereGeometry(0.115, 8, 6).translate(0, 0.105, 0.055),
    };
    case "rocket": {
      const fin = rocketFins();
      const parts: Part[] = [[tube(0.2, 0.82, -0.06), steel],
        [point(0.2, 0.5, 0.6), accent], [tube(0.215, 0.12, -0.46), dark]];
      for (let i = 0; i < 4; i++) parts.push([fin.clone().rotateZ(i * Math.PI / 2), dark]);
      fin.dispose();
      return {
        body: painted(parts), team: tube(0.208, 0.28, -0.08),
        exhaust: rocketExhaust(),
      };
    }
    case "ricochet": return {
      body: painted([[new THREE.CylinderGeometry(0.31, 0.31, 0.16, 6), dark],
        [new THREE.CylinderGeometry(0.24, 0.31, 0.075, 6).translate(0, 0.117, 0), steel],
        [new THREE.TorusGeometry(0.255, 0.037, 4, 12).rotateX(Math.PI / 2).translate(0, 0.15, 0), accent]]),
      team: new THREE.CylinderGeometry(0.19, 0.19, 0.022, 6).translate(0, 0.17, 0),
    };
    case "piercing": return {
      body: painted([[tube(0.085, 0.65, -0.15), dark],
        [point(0.085, 0.55, 0.45), 0xeaffff],
        [new THREE.BoxGeometry(0.34, 0.055, 0.23).translate(0, 0, -0.41), accent],
        [new THREE.BoxGeometry(0.055, 0.28, 0.23).translate(0, 0, -0.41), accent],
        [tube(0.09, 0.25, 0.1), accent]]),
      team: tube(0.095, 0.3, -0.2),
    };
  }
}

export class ProjectileVisuals {
  group = new THREE.Group();
  batches = {} as Record<Weapon, Batch>;
  private pose = new THREE.Object3D();
  private colors = TEAM_COLORS.map(color => new THREE.Color(color));
  constructor() {
    const bodyMaterial = new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.6,
      metalness: 0.15, flatShading: true });
    const teamMaterial = new THREE.MeshBasicMaterial({ color: 0xffffff, toneMapped: false });
    const flameMaterial = new THREE.MeshBasicMaterial({ vertexColors: true, toneMapped: false });
    const layer = (geometry: THREE.BufferGeometry, material: THREE.Material) => {
      const mesh = new THREE.InstancedMesh(geometry, material, CAPACITY);
      mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
      mesh.frustumCulled = false; mesh.count = 0;
      this.group.add(mesh);
      return mesh;
    };
    for (const weapon of AMMO_ORDER) {
      const geometry = model(weapon);
      const size = MODEL_SCALE[weapon];
      for (const part of [geometry.body, geometry.team, geometry.exhaust]) part?.scale(size, size, size);
      this.batches[weapon] = { body: layer(geometry.body, bodyMaterial), team: layer(geometry.team, teamMaterial),
        ...(geometry.exhaust ? { exhaust: layer(geometry.exhaust, flameMaterial) } : {}) };
    }
  }
  reset() {
    for (const mesh of this.group.children as THREE.InstancedMesh[]) mesh.count = 0;
  }
  update(shots: readonly Shot[], time: number) {
    this.reset();
    const pose = this.pose;
    for (let i = 0; i < Math.min(shots.length, CAPACITY); i++) {
      const shot = shots[i], batch = this.batches[shot.weapon], index = batch.body.count++;
      pose.position.set(shot.x, shot.y ?? 1, shot.z);
      pose.rotation.set(0, Math.atan2(shot.vx, shot.vz) + (shot.weapon === "ricochet" ? time * 12 + shot.id : 0), 0);
      pose.scale.setScalar(1); pose.updateMatrix();
      batch.body.setMatrixAt(index, pose.matrix);
      batch.team.setMatrixAt(index, pose.matrix);
      batch.team.setColorAt(index, this.colors[shot.team]);
      batch.team.count++;
      if (batch.exhaust) {
        // A short attached flame, with no persistent trail or per-shot allocations.
        pose.scale.z = 0.92 + 0.08 * Math.sin(time * 47 + shot.id);
        pose.updateMatrix();
        batch.exhaust.setMatrixAt(index, pose.matrix); batch.exhaust.count++;
      }
    }
    for (const mesh of this.group.children as THREE.InstancedMesh[]) {
      if (!mesh.count) continue;
      mesh.instanceMatrix.clearUpdateRanges();
      mesh.instanceMatrix.addUpdateRange(0, mesh.count * 16);
      mesh.instanceMatrix.needsUpdate = true;
      if (mesh.instanceColor) {
        mesh.instanceColor.clearUpdateRanges();
        mesh.instanceColor.addUpdateRange(0, mesh.count * 3);
        mesh.instanceColor.needsUpdate = true;
      }
    }
  }
}
