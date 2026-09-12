import * as THREE from "three";
import { sidingBox } from "./house-surfaces";
import { harborBox } from "./harbor-surfaces";
import { Random } from "./math";
import { box, material, put } from "./model-primitives";
import type { Cover } from "./types";

type CargoShape = Pick<Cover, "w" | "d" | "h" | "color">;
type CrateShape = CargoShape & Pick<Cover, "x" | "z">;
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

export function cargoStack(group: THREE.Group, c: CrateShape, damageStage = 0): void {
  group.userData.damageStage = damageStage;
  // Cosmetic randomness is stable per crate and never advances the simulation RNG.
  const rng = new Random(Math.round(c.x * 73856093 + c.z * 19349663));
  const brokenStrap = rng.next() < 0.5 ? -1 : 1;
  const curlSide = rng.next() < 0.5 ? -1 : 1;
  const curlAngle = rng.range(0.28, 0.55);
  put(group, sidingBox(c.w, c.h - 0.22, c.d, c.color), 0, (c.h + 0.22) / 2, 0);
  // Straps wrap above the lid; flush tops compete with its textured face in the depth buffer.
  const strapBottom = 0.2;
  const strapTop = c.h + 0.06;
  for (const x of [-c.w * 0.34, c.w * 0.34]) {
    put(group, box(0.25, 0.22, c.d, 0x66503a, 0), x, 0.11, 0);
    if (damageStage === 2 && Math.sign(x) === brokenStrap) {
      // A broken top band curls up at its free end; the side bands still hold the box.
      for (const side of [-1, 1]) {
        put(
          group,
          box(0.15, c.h - 0.2, 0.04, 0x6a624d, 0),
          x,
          (c.h + 0.2) / 2,
          side * (c.d / 2 + 0.02),
        );
        put(group, box(0.15, 0.045, c.d * 0.32, 0x6a624d, 0), x, strapTop, side * c.d * 0.34);
      }
      const looseEnd = box(0.15, 0.045, c.d * 0.17, 0x6a624d, 0);
      looseEnd.rotation.x = curlSide * curlAngle;
      put(group, looseEnd, x, strapTop + c.d * 0.035, curlSide * c.d * 0.1);
      continue;
    }
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
  if (damageStage > 0) {
    cargoDamage(group, c, damageStage, rng);
  }
}

// Shared jagged split; light torn fibres surround a darker, narrower recess.
const splitGeometries = Array.from({ length: 4 }, (_, variant) => {
  const rng = new Random(variant + 179);
  return new THREE.ShapeGeometry(
    new THREE.Shape(
      [
        [0, -0.5],
        [-0.22, -0.24],
        [-1, -0.1],
        [-0.34, -0.06],
        [0.15, 0.22],
        [-0.15, 0.5],
        [0.55, 0.23],
        [0.24, 0.03],
        [0.85, -0.08],
        [0.18, -0.03],
        [0.03, -0.25],
      ].map(([x, y]) => new THREE.Vector2(x * rng.range(0.65, 1.35), y)),
    ),
  );
});

function cargoDamage(group: THREE.Group, c: CrateShape, stage: number, rng: Random): void {
  const width = stage === 1 ? 0.095 : 0.17;
  const split = (x: number, y: number, z: number, length: number, rx = 0, ry = 0, rz = 0) => {
    const rotation = new THREE.Euler(rx, ry, rz);
    const normal = new THREE.Vector3(0, 0, 1).applyEuler(rotation);
    const geometry = splitGeometries[Math.floor(rng.next() * splitGeometries.length)];
    const breadth = width * rng.range(0.75, 1.2);
    for (const [i, color] of [0xc9a271, 0x35291c].entries()) {
      const mesh = new THREE.Mesh(geometry, material(color, 0, 0.9));
      mesh.name = "cargo-split";
      mesh.rotation.copy(rotation);
      mesh.scale.set(breadth * (i === 0 ? 1.9 : 1), length, 1);
      // Separate both layers from the wood and each other to avoid flickering.
      put(
        group,
        mesh,
        x + normal.x * i * 0.018,
        y + normal.y * i * 0.018,
        z + normal.z * i * 0.018,
      );
    }
  };
  split(
    c.w * rng.range(-0.15, 0.08),
    c.h + 0.025,
    c.d * rng.range(-0.04, 0.04),
    c.d * rng.range(0.58, 0.8),
    -Math.PI / 2,
    0,
    rng.range(-0.18, 0.18),
  );
  split(
    c.w * rng.range(0.12, 0.24),
    c.h + 0.025,
    c.d * rng.range(-0.2, 0.2),
    c.d * rng.range(0.28, 0.44),
    -Math.PI / 2,
    0,
    rng.range(-0.55, 0.55),
  );
  for (const side of [-1, 1]) {
    split(
      c.w * rng.range(-0.2, 0.2),
      c.h * rng.range(0.48, 0.6),
      side * (c.d / 2 + 0.025),
      c.h * rng.range(0.52, 0.7),
      0,
      side < 0 ? Math.PI : 0,
      rng.range(-0.3, 0.3),
    );
    split(
      side * (c.w / 2 + 0.025),
      c.h * rng.range(0.48, 0.6),
      c.d * rng.range(-0.2, 0.2),
      c.h * rng.range(0.52, 0.7),
      0,
      (side * Math.PI) / 2,
      rng.range(-0.3, 0.3),
    );
  }
  if (stage === 2) {
    // Lift and twist short lid boards over the split, exposing their raw edges.
    const boardCount = rng.next() < 0.5 ? 1 : 2;
    for (let i = 0; i < boardCount; i++) {
      const side = i === 0 ? -1 : 1;
      const board = sidingBox(c.w * 0.22, 0.11, c.d * 0.42, c.color);
      board.rotation.set(rng.range(-0.1, 0.1), rng.range(-0.2, 0.2), side * rng.range(0.08, 0.14));
      put(
        group,
        board,
        side * c.w * rng.range(0.1, 0.17),
        c.h + 0.22,
        c.d * rng.range(-0.18, 0.18),
      );
    }
    const splinterCount = Math.floor(rng.range(2, 5));
    for (let i = 0; i < splinterCount; i++) {
      const splinter = box(0.055, 0.09, c.d * 0.18, 0xd6b17a, 0);
      splinter.rotation.set(0, rng.range(-0.6, 0.6), rng.range(-0.5, 0.5));
      put(group, splinter, c.w * rng.range(-0.24, 0.24), c.h + 0.16, c.d * rng.range(-0.3, 0.3));
    }
  }
}
