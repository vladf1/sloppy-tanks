import * as THREE from "three";
import { batch } from "./batching";
import { concreteWall } from "./concrete-surfaces";
import { shingleRoof, sidingBox, sidingGable } from "./house-surfaces";
import { box, cylinder, material, put } from "./model-primitives";

function beam(group: THREE.Group, a: number[], b: number[], width: number, color = 0x725236) {
  const from = new THREE.Vector3().fromArray(a);
  const to = new THREE.Vector3().fromArray(b);
  const delta = to.clone().sub(from);
  const mesh = box(width, delta.length(), width, color, 0);
  mesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 1, 0), delta.normalize());
  const center = from.add(to).multiplyScalar(0.5);
  put(group, mesh, center.x, center.y, center.z);
}

function footbridge() {
  const group = new THREE.Group();
  group.name = "village-timber-bridge";
  const arch = (z: number) => -0.65 + 2.0 * (1 - (z / 9) ** 2);
  for (let z = -9; z < 9; z += 0.6) {
    const plank = sidingBox(5.4, 0.18, 0.57, 0xb18c5d);
    plank.rotation.x = Math.atan((4 * z) / 81);
    put(group, plank, 0, arch(z), z);
  }
  for (const side of [-1, 1]) {
    for (let z = -9; z <= 9; z += 3) {
      put(group, box(0.22, 1.5, 0.22, 0x6b5036, 0), side * 2.5, arch(z) + 0.75, z);
      put(group, box(0.3, 0.12, 0.3, 0xc4aa7e, 0), side * 2.5, arch(z) + 1.55, z);
      if (z < 9) {
        for (const h of [0.55, 1.25]) {
          beam(group, [side * 2.5, arch(z) + h, z], [side * 2.5, arch(z + 3) + h, z + 3], 0.14);
        }
        beam(group, [side * 2.5, arch(z) + 0.4, z], [side * 2.5, arch(z + 3) + 1.25, z + 3], 0.1);
      }
    }
    put(group, concreteWall(6.1, 2.6, 2.0), 0, -1.85, side * 9.6);
  }
  batch(group);
  return group;
}

function watermill() {
  const group = new THREE.Group();
  const building = new THREE.Group();
  const wheel = new THREE.Group();
  group.name = "pine-watermill";
  wheel.name = "turning-waterwheel";
  put(building, concreteWall(11.4, 3.0, 8.4), 0, 0.4, 0);
  put(building, sidingBox(11, 6.2, 8, 0xac8255), 0, 4.7, 0);
  put(building, sidingGable(12, 3.6, 9, 0x934e3d), 0, 7.8, 0);
  put(building, shingleRoof(12, 3.6, 9, 0x934e3d), 0, 7.8, 0);
  put(building, concreteWall(0.8, 2.4, 0.8), 3, 10.2, -2);
  put(building, box(1.1, 0.2, 1.1, 0x796f5b, 0), 3, 11.5, -2);
  put(building, box(0.53, 0.025, 0.53, 0x34392c, 0), 3, 11.62, -2);
  for (const x of [-5.5, 0, 5.5]) {
    put(building, sidingBox(0.27, 6.4, 0.25, 0x62472e), x, 4.7, 4.04);
    if (x !== 0) {
      put(building, sidingBox(0.27, 6.4, 0.25, 0x62472e), x, 4.7, -4.04);
    }
  }
  for (const y of [2.0, 5.6, 7.65]) {
    put(building, sidingBox(11.3, 0.23, 8.25, 0x62472e), 0, y, 0);
  }
  for (const x of [-3.2, 3.2]) {
    put(building, box(1.8, 1.7, 0.12, 0xe0cc9c, 0), x, 4.15, 4.12);
    put(building, box(1.5, 1.4, 0.1, 0x384f48, 0), x, 4.15, 4.2);
    put(building, box(0.1, 1.4, 0.12, 0xc5b483, 0), x, 4.15, 4.27);
    put(building, box(1.5, 0.1, 0.12, 0xc5b483, 0), x, 4.15, 4.27);
    for (const side of [-1, 1]) {
      const shutter = sidingBox(0.65, 1.8, 0.14, 0x66877b);
      shutter.rotation.y = side * 0.16;
      put(building, shutter, x + side * 1.25, 4.15, 4.13);
    }
    beam(building, [x - 1.7, 5.8, 4.14], [x + 1.7, 7.5, 4.14], 0.18);
  }
  put(building, sidingBox(2.0, 3.2, 0.15, 0x584731), 0, 2.4, 4.17);
  put(building, box(0.13, 0.13, 0.2, 0xc8a963, 0), 0.65, 2.3, 4.3);
  for (let i = 0; i < 4; i++) {
    put(building, concreteWall(3.2, 0.3 + i * 0.15, 0.75), 0, -0.55 + i * 0.19, 6.6 - i * 0.7);
  }
  const awning = shingleRoof(4, 1.0, 2.8, 0x607563);
  put(building, awning, 0, 4.35, 5.2);
  for (const x of [-1.9, 1.9]) {
    put(building, box(0.18, 4.5, 0.18, 0x6c5036, 0), x, 1.9, 6.45);
  }
  // Decorative axle runs through the mill wall to the stream-facing wheel.
  const axle = cylinder(0.27, 6.4, 0x514a3d, 10);
  axle.rotation.x = Math.PI / 2;
  put(building, axle, -8, 1.3, -5.7);
  put(building, concreteWall(2.4, 2.5, 3), -8, -0.3, -4.9);
  put(building, sidingBox(3.2, 2.7, 3.4, 0x88683f), -6.9, 1.85, -3.9);
  put(building, shingleRoof(3.7, 1, 3.9, 0x6c7354), -6.9, 3.2, -3.9);
  for (const side of [-1, 1]) {
    const ring = new THREE.Mesh(
      new THREE.TorusGeometry(4.0, 0.16, 6, 48),
      material(0x514a39, 0.25),
    );
    put(wheel, ring, 0, 0, side * 0.72);
    for (let i = 0; i < 12; i++) {
      const a = (i * Math.PI) / 6;
      beam(
        wheel,
        [0, 0, side * 0.72],
        [Math.cos(a) * 4, Math.sin(a) * 4, side * 0.72],
        0.16,
        0xa98858,
      );
    }
  }
  for (let i = 0; i < 20; i++) {
    const a = (i * Math.PI) / 10;
    const paddle = sidingBox(0.28, 0.72, 1.8, 0x8a6941);
    paddle.rotation.z = a;
    put(wheel, paddle, Math.cos(a) * 4.0, Math.sin(a) * 4.0, 0);
  }
  const hub = cylinder(0.56, 1.8, 0x6a6854, 12);
  hub.rotation.x = Math.PI / 2;
  wheel.add(hub);
  batch(building);
  batch(wheel);
  put(group, building);
  put(group, wheel, -8, 1.3, -9.4);
  return { group, wheel };
}

function logCamp() {
  const group = new THREE.Group();
  group.name = "village-log-cart";
  put(group, sidingBox(4, 0.24, 5, 0x9f7c4e), 0, 1.05, 0);
  for (const x of [-1.8, 1.8]) {
    for (const z of [-1.7, 1.7]) {
      const wheel = cylinder(0.7, 0.23, 0x453d31, 12);
      wheel.rotation.z = Math.PI / 2;
      put(group, wheel, x, 0.68, z);
    }
    beam(group, [x, 0.9, 2.3], [x * 0.55, 0.6, 6], 0.16);
  }
  for (let row = 0; row < 3; row++) {
    for (let i = 0; i < 4 - row; i++) {
      const log = cylinder(0.38, 5.6, 0x80613e, 9);
      log.rotation.x = Math.PI / 2;
      const x = (i - (3 - row) / 2) * 0.8;
      const y = 1.5 + row * 0.65;
      put(group, log, x, y, 0);
      for (const side of [-1, 1]) {
        const end = cylinder(0.31, 0.02, 0xc1a274, 9);
        end.rotation.x = Math.PI / 2;
        put(group, end, x, y, side * 2.81);
      }
    }
  }
  batch(group);
  return group;
}

export function villageLandmarks() {
  const group = new THREE.Group();
  const mill = watermill();
  put(group, mill.group, -35, -0.25, -69);
  put(group, footbridge(), 0, 0, -82);
  const bridge = footbridge();
  bridge.rotation.y = Math.PI / 2;
  put(group, bridge, -85, 0, 38);
  const camp = logCamp();
  camp.rotation.y = 0.17;
  put(group, camp, -68, -0.65, -24);
  return { group, wheel: mill.wheel };
}
