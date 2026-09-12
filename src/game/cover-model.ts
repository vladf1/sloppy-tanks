import * as THREE from "three";
import { explosiveBarrel } from "./barrel-surfaces";
import { concreteWall } from "./concrete-surfaces";
import { cottageDetails } from "./cottage-details";
import { cargoStack, shippingContainer } from "./harbor-models";
import { Random } from "./data";
import { shingleRoof, sidingBox, sidingGable } from "./house-surfaces";
import { box, cylinder, put } from "./model-primitives";
import { TOWER_BASE } from "./tower-layout";
import { treeModel } from "./tree-models";
import type { Cover } from "./types";
function towerFoundation(group: THREE.Group, x: number): void {
  put(
    group,
    concreteWall(TOWER_BASE.width, TOWER_BASE.height, TOWER_BASE.depth),
    x,
    TOWER_BASE.height / 2,
    0,
  );
}
function towerPost(height: number) {
  // Turn the long axis of the boards upright for continuous vertical wood grain.
  const post = sidingBox(height, 0.35, 0.35, 0x887454);
  post.rotation.z = Math.PI / 2;
  return post;
}
export function coverDamageStage(c: Pick<Cover, "kind" | "hp" | "maxHp">): number {
  if (c.kind === "cargo") {
    return c.hp >= c.maxHp ? 0 : c.hp > c.maxHp * 0.35 ? 1 : 2;
  }
  return c.kind === "timber" ? Math.min(2, Math.floor(((c.maxHp - c.hp) * 3) / c.maxHp)) : 0;
}
export function coverModel(
  c: Pick<Cover, "kind" | "x" | "z" | "w" | "d" | "h" | "color" | "debrisSeed">,
  detail: "full" | "background" = "full",
  damageStage = 0,
) {
  if (c.kind === "tree") {
    return treeModel(c, detail);
  }
  const group = new THREE.Group();
  group.position.set(c.x, 0, c.z);
  if (c.kind === "container") {
    shippingContainer(group, c);
  } else if (c.kind === "cargo") {
    cargoStack(group, c, damageStage);
  } else if (c.kind === "house") {
    const wall = c.h * 0.68;
    put(group, box(c.w + 0.2, 0.22, c.d + 0.2, 0xa1977c, 0), 0, 0.11, 0);
    put(group, sidingBox(c.w, wall, c.d, c.color), 0, wall / 2, 0);
    // Pale corner boards and a stone sill frame the clapboard walls.
    for (const x of [-1, 1]) {
      for (const z of [-1, 1]) {
        put(group, box(0.14, wall, 0.14, 0xd4be95, 0), (x * c.w) / 2, wall / 2, (z * c.d) / 2);
      }
    }
    for (const side of [-1, 1]) {
      put(group, box(c.w + 0.16, 0.16, 0.12, 0x856447, 0), 0, 0.28, (side * c.d) / 2);
      put(group, box(0.12, 0.16, c.d + 0.16, 0x856447, 0), (side * c.w) / 2, 0.28, 0);
    }
    for (const side of [-1, 1]) {
      for (const x of [-c.w * 0.29, c.w * 0.29]) {
        put(group, box(1.24, 1.16, 0.1, 0xe5cea1, 0), x, wall * 0.59, side * (c.d / 2 + 0.025));
        put(
          group,
          box(1.36, 0.1, 0.25, 0xc8b087, 0),
          x,
          wall * 0.59 - 0.6,
          side * (c.d / 2 + 0.09),
        );
        for (const shutter of [-1, 1]) {
          put(
            group,
            box(0.22, 1.05, 0.1, 0x4d6650, 0),
            x + shutter * 0.75,
            wall * 0.59,
            side * (c.d / 2 + 0.06),
          );
          for (const y of [-0.3, 0, 0.3]) {
            put(
              group,
              box(0.24, 0.035, 0.11, 0x334a3c, 0),
              x + shutter * 0.75,
              wall * 0.59 + y,
              side * (c.d / 2 + 0.08),
            );
          }
        }
        put(group, box(1.05, 0.97, 0.07, 0xffd94e, 0), x, wall * 0.59, side * (c.d / 2 + 0.045));
        put(group, box(0.075, 0.97, 0.085, 0x875534, 0), x, wall * 0.59, side * (c.d / 2 + 0.09));
        put(group, box(1.05, 0.075, 0.085, 0x875534, 0), x, wall * 0.59, side * (c.d / 2 + 0.09));
      }
      put(group, box(0.07, 1.05, 1.1, 0xffd94e, 0), side * (c.w / 2 + 0.05), wall * 0.58, 0);
    }
    for (const side of [-1, 1]) {
      put(group, box(0.08, 1.22, 1.28, 0xe5cea1, 0), side * (c.w / 2 + 0.01), wall * 0.58, 0);
      put(group, box(0.1, 1.05, 0.07, 0x875534, 0), side * (c.w / 2 + 0.09), wall * 0.58, 0);
      put(group, box(0.1, 0.07, 1.1, 0x875534, 0), side * (c.w / 2 + 0.09), wall * 0.58, 0);
      put(group, box(0.25, 0.1, 1.36, 0xc8b087, 0), side * (c.w / 2 + 0.07), wall * 0.58 - 0.65, 0);
    }
    put(group, box(1.03, 1.72, 0.11, 0xe5cea1, 0), 0, 0.88, c.d / 2 + 0.015);
    put(group, box(1.2, 0.18, 0.62, 0x9a9585, 0), 0, 0.14, c.d / 2 + 0.2);
    put(group, box(0.82, 1.55, 0.1, 0x64452f, 0), 0, 0.85, c.d / 2 + 0.06);
    put(group, box(0.1, 0.1, 0.12, 0xffd24a, 0), 0.24, 0.83, c.d / 2 + 0.12);
    for (const y of [0.5, 1.15]) {
      put(group, box(0.6, 0.42, 0.035, 0x805b3d, 0), 0, y, c.d / 2 + 0.12);
    }
    // Gentle paint weathering varies per cottage without splitting material batches.
    const roofColor = new THREE.Color(Math.abs(c.z) > 35 ? 0xcc493c : 0x167857)
      .multiplyScalar(0.9 + 0.12 * (0.5 + 0.5 * Math.sin(c.x * 3.7 + c.z * 1.9)))
      .getHex();
    put(group, sidingGable(c.w + 0.6, c.h - wall, c.d + 0.6, roofColor), 0, wall, 0);
    put(group, shingleRoof(c.w + 0.6, c.h - wall, c.d + 0.6, roofColor), 0, wall, 0);
    for (const side of [-1, 1]) {
      put(group, box(0.16, 0.15, c.d + 0.7, 0xe0c79d, 0), (side * (c.w + 0.6)) / 2, wall, 0);
    }
    for (let z = -(c.d + 0.6) / 2; z < (c.d + 0.6) / 2; z += 0.48) {
      put(
        group,
        box(0.22, 0.11, Math.min(0.46, (c.d + 0.6) / 2 - z), 0x334a40, 0),
        0,
        c.h + 0.04,
        z + 0.23,
      );
    }
    put(group, box(0.74, 0.14, 0.74, 0x705a4d, 0), -c.w * 0.25, c.h + 0.19, -c.d * 0.2);
    put(group, box(0.43, 0.015, 0.43, 0x302c29, 0), -c.w * 0.25, c.h + 0.27, -c.d * 0.2);
    for (let y = c.h - 0.75; y < c.h + 0.12; y += 0.22) {
      put(group, box(0.59, 0.026, 0.59, 0xd3b095, 0), -c.w * 0.25, y, -c.d * 0.2);
    }
    put(group, box(0.58, 1.0, 0.58, 0xbc5c3e, 0), -c.w * 0.25, c.h - 0.36, -c.d * 0.2);
    cottageDetails(group, c);
  } else if (c.kind === "timber") {
    group.userData.damageStage = damageStage;
    const along = c.w > c.d;
    const length = Math.max(c.w, c.d);
    const colors = [c.color, 0x94613e, 0xa66f46];
    // Closely stacked beams stay opaque at shell height, even when chipped.
    for (let row = 0; row < 7; row++) {
      const chipped = damageStage > 0 && row >= 7 - damageStage * 2;
      const span = length - (chipped ? 0.35 + (row % 2) * 0.4 : 0.04);
      const beam = box(
        along ? span : 0.64,
        0.38,
        along ? 0.64 : span,
        damageStage === 2 ? 0x795035 : colors[row % 3],
        0,
      );
      put(
        group,
        beam,
        along && chipped ? (row % 2 ? -0.16 : 0.16) : 0,
        0.2 + row * 0.39,
        !along && chipped ? (row % 2 ? -0.16 : 0.16) : 0,
      );
    }
    for (const offset of [-length / 2 + 0.22, length / 2 - 0.22]) {
      put(
        group,
        box(along ? 0.3 : 0.9, c.h, along ? 0.9 : 0.3, 0x805336, 0),
        along ? offset : 0,
        c.h / 2,
        along ? 0 : offset,
      );
      for (const y of [0.6, 2.15]) {
        put(
          group,
          box(along ? 0.32 : 0.92, 0.09, along ? 0.92 : 0.32, 0x49423a, 0),
          along ? offset : 0,
          y,
          along ? 0 : offset,
        );
      }
    }
  } else if (c.kind === "fence") {
    const along = c.w > c.d;
    const length = Math.max(c.w, c.d);
    for (let offset = -length / 2 + 0.12; offset <= length / 2; offset += 0.55) {
      put(
        group,
        box(along ? 0.24 : 0.18, c.h, along ? 0.18 : 0.24, c.color, 0),
        along ? offset : 0,
        c.h / 2,
        along ? 0 : offset,
      );
    }
    for (const y of [0.45, 1.12]) {
      put(group, box(along ? length : 0.2, 0.18, along ? 0.2 : length, 0x8f603a, 0), 0, y, 0);
    }
  } else if (c.kind === "drum") {
    put(group, explosiveBarrel(), 0, 0.8, 0);
    for (const y of [0.22, 1.35]) {
      put(group, cylinder(0.63, 0.1, 0x574e3e), 0, y, 0);
    }
    put(group, cylinder(0.15, 0.05, 0x343c31), 0.25, 1.63, 0);
  } else if (c.kind === "tower") {
    for (const side of [-1, 1]) {
      const x = side * TOWER_BASE.offset;
      towerFoundation(group, x);
      for (const z of [-TOWER_BASE.postZ, TOWER_BASE.postZ]) {
        put(group, towerPost(4.3), x, TOWER_BASE.height + 2.15, z);
      }
      // Cross bracing terminates at the same posts that survive the collapse.
      for (const direction of [-1, 1]) {
        const brace = sidingBox(0.18, 4.35, 0.18, 0x96734c);
        brace.rotation.x = direction * Math.atan2(2 * TOWER_BASE.postZ, 3.8);
        put(group, brace, x, 2.85, 0);
      }
    }
    put(group, sidingBox(6, 0.35, 5, 0x887d59), 0, 5, 0);
    put(group, sidingBox(5.7, 2.15, 4.7, c.color), 0, 6.15, 0);
    for (const z of [-2.4, 2.4]) {
      put(group, box(4, 0.65, 0.08, 0x164e79), 0, 6.4, z);
    }
    put(group, sidingGable(6.5, 1.2, 5.5, 0x197451), 0, 7.25, 0);
    put(group, shingleRoof(6.5, 1.2, 5.5, 0x197451), 0, 7.25, 0);
    for (const x of [2.2, 3.1]) {
      put(group, towerPost(4.9), x, 2.45, 2.15);
    }
    for (let i = 0; i < 9; i++) {
      put(group, sidingBox(0.9, 0.08, 0.18, 0xe2cc93), 2.65, 0.4 + i * 0.55, 2.15);
    }
  } else if (c.kind === "rubble") {
    towerFoundation(group, 0);
    const rng = new Random(c.debrisSeed ?? Math.round(c.x * 73856093 + c.z * 19349663));
    const choose = (values: number[]) => values[Math.floor(rng.next() * values.length)];
    for (const z of [-TOWER_BASE.postZ, TOWER_BASE.postZ]) {
      // Cut posts keep their original position, section and grain direction.
      const height = choose([0.12, 0.2, 0.28, 0.34]);
      put(group, towerPost(height), 0, TOWER_BASE.height + height / 2, z);
      if (rng.next() < 0.7) {
        const splinter = sidingBox(0.09, 0.12, 0.16, 0xc5a073);
        splinter.rotation.z = rng.range(-0.4, 0.4);
        put(group, splinter, rng.range(-0.1, 0.1), TOWER_BASE.height + height - 0.01, z);
      }
    }
    // Discrete sizes reuse cached geometry; each foundation gets its own scatter.
    const count = choose([2, 3, 4]);
    for (let i = 0; i < count; i++) {
      const width = choose([0.16, 0.3, 0.55]);
      const length = choose([0.7, 1.1, 1.5]);
      const yaw = rng.range(-0.55, 0.55);
      const board = sidingBox(width, 0.09, length, choose([c.color, 0x887d59, 0x96734c]));
      board.rotation.y = yaw;
      // Keep the pile inside its foundation, preserving the opened center route.
      const roomX = Math.max(
        0,
        (TOWER_BASE.width - width * Math.cos(yaw) - length * Math.abs(Math.sin(yaw))) / 2 - 0.02,
      );
      const roomZ =
        (TOWER_BASE.depth - length * Math.cos(yaw) - width * Math.abs(Math.sin(yaw))) / 2 - 0.02;
      put(
        group,
        board,
        rng.range(-roomX, roomX),
        TOWER_BASE.height + 0.045 + i * 0.055,
        rng.range(-roomZ, roomZ),
      );
    }
  } else if (c.kind === "shed") {
    put(group, box(c.w, c.h, c.d, c.color), 0, c.h / 2, 0);
    const along = c.w > c.d;
    const length = along ? c.w : c.d;
    for (let i = -length / 2 + 0.2; i < length / 2; i += 0.52) {
      put(
        group,
        box(along ? 0.035 : c.w + 0.04, c.h - 0.1, along ? c.d + 0.04 : 0.035, 0x9c713e, 0.005),
        along ? i : 0,
        c.h / 2,
        along ? 0 : i,
      );
    }
    put(group, box(c.w + 0.14, 0.18, c.d + 0.14, 0x187fbe), 0, c.h, 0);
  } else {
    put(
      group,
      c.kind === "boundary" ? concreteWall(c.w, c.h, c.d) : box(c.w, c.h, c.d, c.color, 0.16),
      0,
      c.h / 2,
      0,
    );
    if (c.kind === "concrete" || c.kind === "wall") {
      const along = c.w > c.d;
      for (let i = 0; i < Math.floor((along ? c.w : c.d) / 1.1); i++) {
        const mark = box(along ? 0.55 : 0.035, 0.22, along ? 0.035 : 0.55, 0x499ac7, 0.01);
        put(
          group,
          mark,
          along ? -c.w / 2 + 0.6 + i * 1.1 : c.w / 2 + 0.02,
          c.h * 0.7,
          along ? c.d / 2 + 0.02 : -c.d / 2 + 0.6 + i * 1.1,
        );
      }
    }
  }
  return group;
}
