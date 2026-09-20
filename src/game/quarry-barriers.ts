import * as THREE from "three";
import { concreteWall } from "./concrete-surfaces";
import { harborBox } from "./harbor-surfaces";
import { cylinder, material, put } from "./model-primitives";
import {
  dragonToothPoint,
  dragonToothProfile,
  dragonToothVariant,
  HEDGEHOG_BEAMS,
} from "./quarry-barrier-shapes";

const teeth = new Map<string, THREE.BufferGeometry>();
let toothMaterial: THREE.MeshStandardMaterial | undefined;
const liftingArch = new THREE.TorusGeometry(0.105, 0.022, 6, 12, Math.PI);

/** Separate precast blocks: weathered faces, soil-darkened feet and exposed rebar eyes. */
export function dragonTooth(
  group: THREE.Group,
  w: number,
  h: number,
  d: number,
  x: number,
  z: number,
): void {
  const variant = dragonToothVariant(x, z);
  const key = `${w}/${h}/${d}/${variant}`;
  let geometry = teeth.get(key);
  if (!geometry) {
    geometry = new THREE.BoxGeometry(1, 1, 1, 1, 4, 1);
    const p = geometry.getAttribute("position");
    const uv = geometry.getAttribute("uv");
    const colors: number[] = [];
    const concrete = new THREE.Color(0xd2d2c9).multiplyScalar(0.97 + variant * 0.02);
    const soil = new THREE.Color(0x8b816c);
    for (let i = 0; i < p.count; i++) {
      const y = p.getY(i);
      const point = dragonToothPoint(p.getX(i), y, p.getZ(i), w, h, d, variant);
      p.setXYZ(i, point[0], point[1], point[2]);
      uv.setXY(i, (uv.getX(i) * w) / 1.4 + variant * 0.23, (uv.getY(i) * h) / 1.4);
      const tint = concrete.clone().lerp(soil, Math.max(0, 1 - (y + 0.5) * 5) * 0.5);
      colors.push(tint.r, tint.g, tint.b);
    }
    geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
    geometry.computeVertexNormals();
    teeth.set(key, geometry);
  }
  if (!toothMaterial) {
    toothMaterial = concreteWall(1, 1, 1).material.clone();
    toothMaterial.vertexColors = true;
    toothMaterial.bumpScale = 0.055;
  }
  const mesh = new THREE.Mesh(geometry, toothMaterial);
  mesh.castShadow = mesh.receiveShadow = true;
  put(group, mesh, 0, h / 2, 0);

  const { yaw, topScale } = dragonToothProfile(variant);
  if (topScale === 0) {
    return;
  }
  const arch = new THREE.Mesh(liftingArch, material(0x625447, 0.55, 0.85));
  arch.rotation.y = yaw;
  arch.castShadow = arch.receiveShadow = true;
  put(group, arch, 0, h + 0.08, 0);
  for (const side of [-1, 1]) {
    const stem = cylinder(0.022, 0.18, 0x625447, 6);
    stem.material = arch.material;
    put(group, stem, side * 0.105 * Math.cos(yaw), h - 0.01, -side * 0.105 * Math.sin(yaw));
  }
}

/** Three crossed I-sections with real flange depth and bolted connecting plates. */
export function steelHedgehog(group: THREE.Group): void {
  for (const beam of HEDGEHOG_BEAMS) {
    const assembly = new THREE.Group();
    put(assembly, harborBox(0.12, beam.length, 0.44, 0x726454));
    for (const x of [-0.22, 0.22]) {
      put(assembly, harborBox(0.1, beam.length, 0.52, 0x625b50), x);
    }
    assembly.rotation.set(beam.rx, 0, beam.rz);
    put(group, assembly, 0, 1.3, 0);
    // Bake assemblies into the cover's batch without retaining separate draw calls.
    assembly.updateMatrix();
    for (const mesh of [...assembly.children]) {
      mesh.applyMatrix4(assembly.matrix);
      group.add(mesh);
    }
    group.remove(assembly);
  }
  for (const z of [-0.29, 0.29]) {
    put(group, harborBox(0.64, 0.64, 0.08, 0x625b50), 0, 1.3, z);
    for (const x of [-0.2, 0.2]) {
      for (const y of [1.1, 1.5]) {
        const bolt = cylinder(0.055, 0.1, 0x999083, 6);
        bolt.rotation.x = Math.PI / 2;
        put(group, bolt, x, y, z);
      }
    }
  }
}
