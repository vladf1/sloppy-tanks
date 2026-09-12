import * as THREE from "three";
import { sidingBox } from "./house-surfaces";
import { harborBox } from "./harbor-surfaces";
import { box, put } from "./model-primitives";
import type { Cover } from "./types";

type CargoShape = Pick<Cover, "w" | "d" | "h" | "color">;
/** Corrugated steel, corner castings, double doors and locking bars share cached meshes. */
export function shippingContainer(group: THREE.Group, c: CargoShape): void {
  const alongZ = c.d > c.w;
  const width = alongZ ? c.d : c.w;
  const depth = alongZ ? c.w : c.d;
  const part = (
    w: number,
    h: number,
    d: number,
    color: number,
    x: number,
    y: number,
    z: number,
  ) => {
    const mesh = w * h * d > 1 ? harborBox(w, h, d, color) : box(w, h, d, color, 0.025);
    if (alongZ) {
      mesh.rotation.y = Math.PI / 2;
    }
    put(group, mesh, alongZ ? z : x, y, alongZ ? -x : z);
  };
  part(width, c.h, depth, c.color, 0, c.h / 2, 0);
  const dark = new THREE.Color(c.color).multiplyScalar(0.67).getHex();
  for (const s of [-1, 1]) {
    for (let x = -width / 2 + 0.4; x < width / 2 - 0.2; x += 0.55) {
      part(0.12, c.h - 0.35, 0.09, dark, x, c.h / 2, s * (depth / 2 + 0.025));
    }
    for (const y of [0.12, c.h - 0.12]) {
      part(width + 0.1, 0.18, 0.15, dark, 0, y, (s * depth) / 2);
      part(0.18, 0.18, depth, dark, (s * width) / 2, y, 0);
    }
    for (const z of [-depth / 2 + 0.13, depth / 2 - 0.13]) {
      // Keep corner caps above the roof and its ribs; coplanar tops flicker as the camera moves.
      const postHeight = c.h + 0.08;
      part(0.22, postHeight, 0.22, 0xc4c4ad, s * (width / 2 - 0.1), postHeight / 2, z);
    }
    // Recessed double-door panels and silver locking rods on both ends.
    for (const z of [-depth / 4, depth / 4]) {
      part(0.08, c.h - 0.55, depth / 2 - 0.15, dark, s * (width / 2 + 0.025), c.h / 2, z);
      part(0.13, c.h - 0.6, 0.08, 0xc4c4ad, s * (width / 2 + 0.08), c.h / 2, z);
      part(0.16, 0.09, 0.5, 0xd6d1b2, s * (width / 2 + 0.1), 1.15, z);
    }
    part(2.0, 0.5, 0.035, 0xe1d9b7, -width * 0.27, c.h * 0.65, s * (depth / 2 + 0.09));
    for (let i = 0; i < 4; i++) {
      part(
        0.09,
        0.24,
        0.045,
        dark,
        -width * 0.27 - 0.5 + i * 0.3,
        c.h * 0.65,
        s * (depth / 2 + 0.12),
      );
    }
  }
  for (let x = -width / 2 + 0.4; x < width / 2; x += 0.55) {
    part(0.12, 0.04, depth - 0.3, dark, x, c.h + 0.015, 0);
  }
}

export function cargoStack(group: THREE.Group, c: CargoShape): void {
  put(group, sidingBox(c.w, c.h - 0.22, c.d, c.color), 0, (c.h + 0.22) / 2, 0);
  // Straps wrap above the lid; flush tops compete with its textured face in the depth buffer.
  const strapBottom = 0.2;
  const strapTop = c.h + 0.06;
  for (const x of [-c.w * 0.34, c.w * 0.34]) {
    put(group, box(0.25, 0.22, c.d, 0x66503a, 0), x, 0.11, 0);
    put(
      group,
      box(0.15, strapTop - strapBottom, c.d + 0.04, 0x6a624d, 0),
      x,
      (strapTop + strapBottom) / 2,
      0,
    );
  }
  for (const z of [-1, 1]) {
    for (const y of [0.42, c.h - 0.2]) {
      put(group, sidingBox(c.w, 0.2, 0.12, 0xdec18b), 0, y, z * (c.d / 2 + 0.04));
    }
    const brace = sidingBox(c.w * 0.9, 0.2, 0.12, 0xdec18b);
    brace.rotation.z = 0.58;
    put(group, brace, 0, c.h / 2, z * (c.d / 2 + 0.08));
  }
}
