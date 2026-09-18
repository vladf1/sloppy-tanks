import { ARENA } from "./data";
import type { CoverKind, PickupKind, Team, Vec2 } from "./types";
import { TIMBER_HEALTH, type TimberJoin } from "./timber-layout";
export interface CoverDef extends Vec2 {
  kind: CoverKind;
  w: number;
  d: number;
  h: number;
  hp: number;
  color: number;
  timberJoin?: TimberJoin;
  timberBays?: number;
}
function authoredLayout(): CoverDef[] {
  const result: CoverDef[] = [];
  const add = (
    kind: CoverKind,
    x: number,
    z: number,
    w: number,
    d: number,
    h: number,
    hp: number,
    color: number,
  ) => result.push({ kind, x, z, w, d, h, hp, color });
  for (const s of [-1, 1]) {
    add("boundary", s * (ARENA + 0.5), 0, 1, ARENA * 2 + 2, 2.2, Infinity, 0xa68c68);
    add("boundary", 0, s * (ARENA + 0.5), ARENA * 2 + 2, 1, 2.2, Infinity, 0xa68c68);
    for (const z of [-45, -9, 9, 45]) {
      add("tree", s * 36, z, 2.6, 2.6, 5.8, 80, 0x169f65);
    }
    for (const z of [-39, -13, 13, 39]) {
      add("house", s * 45, z, 5, 6, 4.6, Infinity, 0xb87b4c);
    }
    for (const z of [-46, 46]) {
      add("house", s * 17, z, 7, 5, 5.2, Infinity, 0xc78b50);
    }
    for (const z of [-28, 28]) {
      add("tree", s * 23, z, 2.6, 2.6, 6, 80, 0x169f65);
      // Open cottage gardens provide flanking space; each timber bay breaks independently.
      const depth = 0.9;
      const backX = 33.2;
      const frontX = 24.5;
      // A single square upright joins each corner. Runs stop at its faces.
      const sideEnd = backX - depth / 2 - 0.04;
      for (const end of [-1, 1]) {
        result.push({
          kind: "timber",
          x: (s * (frontX + sideEnd)) / 2,
          z: z + end * 5,
          w: sideEnd - frontX,
          d: depth,
          h: 2.8,
          hp: TIMBER_HEALTH,
          color: 0xb47a49,
          timberJoin: s > 0 ? { openMax: true } : { openMin: true },
          timberBays: 2,
        });
        result.push({
          kind: "timber",
          x: s * backX,
          z: z + end * 5,
          w: depth,
          d: depth,
          h: 2.8,
          hp: TIMBER_HEALTH,
          color: 0x805336,
          timberJoin: { post: true },
        });
      }
      result.push({
        kind: "timber",
        x: s * backX,
        z,
        w: depth,
        d: 10 - depth - 0.08,
        h: 2.8,
        hp: TIMBER_HEALTH,
        color: 0xb47a49,
        timberJoin: { openMin: true, openMax: true },
        timberBays: 3,
      });
      add("drum", s * 25, z - s, 1.2, 1.2, 1.7, 30, 0xff5b24);
    }
    for (const x of [-6, -2, 2, 6]) {
      add("timber", x, s * 13, 3.7, 0.9, 2.8, TIMBER_HEALTH, 0xb47a49);
    }
    add("tower", s * 12.75, -s * 28, 6, 5, 7.5, 180, 0xbd864a);
    for (const z of [-2, 2]) {
      add("drum", s * 6, z, 1.2, 1.2, 1.7, 30, 0xff5b24);
    }
    add("house", s * 24, 0, 5, 7, 4.9, 180, 0xb87b4c);
  }
  return result;
}
/** Each timber bay can break independently. */
function segmentWalls(layout: CoverDef[]): CoverDef[] {
  return layout.flatMap((c) => {
    if (c.kind !== "timber" || c.timberJoin?.post) {
      return [c];
    }
    const along = c.w > c.d;
    const length = Math.max(c.w, c.d);
    const count = c.timberBays ?? Math.max(1, Math.round(length / 3.7));
    const span = length / count;
    return Array.from({ length: count }, (_, i) => {
      const offset = (i - (count - 1) / 2) * span;
      return {
        ...c,
        x: c.x + (along ? offset : 0),
        z: c.z + (along ? 0 : offset),
        w: along ? span : c.w,
        d: along ? c.d : span,
        timberJoin: c.timberJoin
          ? {
              openMin: i === 0 && c.timberJoin.openMin,
              openMax: i === count - 1 && c.timberJoin.openMax,
            }
          : undefined,
      };
    });
  });
}
export function arenaLayout(): CoverDef[] {
  return segmentWalls(authoredLayout());
}
export const pickupLayout: { kind: PickupKind; x: number; z: number }[] = [
  // One rare, contested pickup at the rotationally symmetric center.
  { kind: "laser", x: 0, z: 0 },
  { kind: "rapid", x: 0, z: -36 },
  { kind: "rapid", x: 0, z: 36 },
  // Four distinct route pairs, mirrored by 180 degrees for equal team access.
  { kind: "spread", x: -38, z: -22 },
  { kind: "spread", x: 38, z: 22 },
  { kind: "rocket", x: -16, z: -18 },
  { kind: "rocket", x: 16, z: 18 },
  { kind: "ricochet", x: -18, z: 36 },
  { kind: "ricochet", x: 18, z: -36 },
  { kind: "piercing", x: -38, z: 22 },
  { kind: "piercing", x: 38, z: -22 },
  { kind: "repair", x: -38, z: 0 },
  { kind: "repair", x: 38, z: 0 },
  { kind: "repair", x: 0, z: -52 },
  { kind: "repair", x: 0, z: 52 },
  { kind: "shield", x: -29, z: 46 },
  { kind: "shield", x: 29, z: -46 },
  { kind: "speed", x: -29, z: -46 },
  { kind: "speed", x: 29, z: 46 },
];
export const spawnPositions = (team: Team): Vec2[] =>
  [-46, -23, 0, 23, 46].map((z) => ({
    x: team === 0 ? -53 : 53,
    z: team === 0 ? z : -z,
  }));
