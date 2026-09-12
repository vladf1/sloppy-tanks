import * as THREE from "three";
import { box, material, put } from "./model-primitives";
import type { Cover } from "./types";

const leaves = new THREE.OctahedronGeometry(1);
const blossom = new THREE.OctahedronGeometry(1);
const soil = new THREE.PlaneGeometry(1.2, 0.28).rotateX(-Math.PI / 2);
/** Attached garden details disappear with the cottage, rather than leaving floating planters. */
export function cottageDetails(group: THREE.Group, c: Pick<Cover, "w" | "d" | "h" | "x" | "z">) {
  const wall = c.h * 0.68;
  const flowers = [0xddb1ac, 0xf2d893, 0xc5b4d0];
  const color = flowers[Math.abs(Math.round(c.x + c.z)) % flowers.length];
  for (const side of [-1, 1]) {
    for (const x of [-c.w * 0.29, c.w * 0.29]) {
      const z = side * (c.d / 2 + 0.28);
      const y = wall * 0.59 - 0.79;
      put(group, box(1.34, 0.23, 0.38, 0x947047, 0), x, y, z);
      put(group, new THREE.Mesh(soil, material(0x493f2d, 0, 1)), x, y + 0.13, z);
      for (const offset of [-0.34, 0.34]) {
        const bush = new THREE.Mesh(leaves, material(0x3d703e, 0, 1));
        bush.scale.set(0.35, 0.17, 0.19);
        put(group, bush, x + offset, y + 0.22, z);
        const flower = new THREE.Mesh(blossom, material(color, 0, 0.9));
        flower.scale.set(0.1, 0.065, 0.1);
        put(group, flower, x + offset * 0.8, y + 0.37, z + side * 0.05);
      }
    }
  }
}
