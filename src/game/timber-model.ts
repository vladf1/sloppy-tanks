import * as THREE from "three";
import { Random } from "./math";
import { batch } from "./batching";
import { box, material, put } from "./model-primitives";
import type { TimberPart } from "./timber-layout";

/** Reused for standing beams and detached pieces, including chips, cracks and attached straps. */
export function timberPartModel(p: TimberPart): THREE.Group {
  const group = new THREE.Group();
  const owned: THREE.BufferGeometry[] = [];
  put(group, box(p.w, p.h, p.d, p.color, 0));
  for (const [markIndex, mark] of p.marks.entries()) {
    const rng = new Random(mark.seed);
    const end = mark.face === "left" || mark.face === "right";
    const sign = mark.face === "left" || mark.face === "back" ? -1 : 1;
    const span = end ? p.d : p.w;
    const x = THREE.MathUtils.clamp(mark.x, -span / 2 + 0.01, span / 2 - 0.01);
    // Keep the scar close to impact while avoiding a half-mark clipped along a beam seam.
    const margin = Math.min(p.h * 0.3, 0.13 * mark.size);
    const y = THREE.MathUtils.clamp(mark.y, -p.h / 2 + margin, p.h / 2 - margin);
    const plane = (points: number[][], color: number, layer: number) => {
      // Clip scars at the piece edges; no cracks floating beyond the wood.
      const outline = points.map(
        (point) =>
          new THREE.Vector2(
            (end ? -sign : sign) *
              THREE.MathUtils.clamp(x + point[0], -span / 2 + 0.005, span / 2 - 0.005),
            THREE.MathUtils.clamp(y + point[1], -p.h / 2 + 0.005, p.h / 2 - 0.005),
          ),
      );
      const geo = new THREE.ShapeGeometry(new THREE.Shape(outline));
      geo.rotateY(end ? (sign * Math.PI) / 2 : sign < 0 ? Math.PI : 0);
      owned.push(geo);
      const offset = 0.003 + markIndex * 0.001 + layer * 0.001;
      put(
        group,
        new THREE.Mesh(geo, material(color)),
        end ? sign * (p.w / 2 + offset) : 0,
        0,
        end ? 0 : sign * (p.d / 2 + offset),
      );
    };
    const width = 0.19 * mark.size * rng.range(0.85, 1.15);
    const height = 0.075 * mark.size * rng.range(0.85, 1.15);
    const chip: number[][] = [];
    for (let i = 0; i < 10; i++) {
      const angle = (i * Math.PI) / 5;
      const radius = rng.range(0.65, 1);
      chip.push([Math.cos(angle) * width * radius, Math.sin(angle) * height * radius]);
    }
    plane(chip, 0xc59b65, 0);
    plane(
      chip.map(([u, v]) => [u * 0.65 + 0.02, v * 0.65 + 0.012]),
      0x805334,
      1,
    );
    // Every impact owns its random path and fixed size, independent of later HP stages.
    const reach = 0.38 * mark.size * rng.range(0.85, 1.15);
    const slope = rng.range(-0.12, 0.12);
    const count = 5 + Math.floor(rng.next() * 3);
    const points: number[][] = [];
    for (let i = 0; i <= count; i++) {
      const u = -reach + (2 * reach * i) / count;
      points.push([u, u * slope + rng.range(-0.045, 0.045)]);
    }
    const split = (path: number[][], thickness: number) => {
      for (const lip of [true, false]) {
        const outline: number[][] = [];
        for (const side of [1, -1]) {
          for (let j = 0; j < path.length; j++) {
            const i = side > 0 ? j : path.length - 1 - j;
            const taper = Math.sin((Math.PI * i) / (path.length - 1));
            outline.push([
              path[i][0],
              path[i][1] + side * (thickness + (lip ? 0.004 : 0)) * taper - (lip ? 0.006 : 0),
            ]);
          }
        }
        plane(outline, lip ? 0xb98a55 : 0x503421, lip ? 2 : 3);
      }
    };
    split(points, 0.012 * mark.size);
    const root = points[2 + Math.floor(rng.next() * (count - 2))];
    const direction = rng.next() < 0.5 ? -1 : 1;
    split(
      [
        root,
        [root[0] + reach * 0.2, root[1] + direction * 0.08],
        [root[0] + reach * rng.range(0.35, 0.6), root[1] + direction * 0.16 * mark.size],
      ],
      0.008 * mark.size,
    );
  }
  if (p.kind === "post") {
    for (const fraction of [-0.28, 0.28]) {
      const strap = box(p.w + 0.024, 0.085, p.d + 0.025, 0x49423a, 0);
      if (p.damage === 3 && fraction > 0) {
        strap.rotation.z = 0.13;
      }
      put(group, strap, 0, p.h * fraction, p.damage === 3 && fraction > 0 ? 0.025 : 0);
    }
  }
  batch(group);
  for (const geometry of owned) {
    geometry.dispose();
  }
  return group;
}

export function addTimberParts(group: THREE.Group, parts: TimberPart[]): void {
  const wallHeight = Math.max(...parts.map((part) => part.h));
  // Hide the debris clearance inside the uprights while the fence is standing.
  const endOverlap = 0.04 + wallHeight * 0.02 + 0.01;
  for (const part of parts) {
    const assembly = timberPartModel(
      part.kind === "beam" ? { ...part, w: part.w + endOverlap * 2 } : part,
    );
    assembly.position.set(part.x, part.y, part.z);
    assembly.rotation.set(0, part.yaw, part.lean);
    assembly.updateMatrix();
    for (const child of [...assembly.children]) {
      child.applyMatrix4(assembly.matrix);
      // These temporary batches are consumed by the cover's final static batch.
      group.add(child);
    }
  }
  group.userData.timberParts = parts;
}
