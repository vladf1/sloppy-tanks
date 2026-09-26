import type { CoverDef } from "./game/arena";
import { ARENA } from "./game/data";
import { DEBRIS_CLEANUP_SECONDS } from "./game/debris-cleanup";
import type { ArenaMap } from "./game/maps";
import type { Simulation, SimulationSetup } from "./game/simulation";
import { TIMBER_HEALTH } from "./game/timber-layout";
import type { Cover, CoverKind } from "./game/types";
import { STRESS_TEST_SETUP } from "./stress-test-level";

/** Two-thirds of the standard arena's width: the same 30 tanks fight at 2.4x the density. */
export const SUPERSTRESS_SCALE = 0.65;
const YARD = ARENA * SUPERSTRESS_SCALE;
/** Three times the normal debris budget; FRAGMENT_CAPACITY bounds what presentation draws. */
export const SUPERSTRESS_MAX_FRAGMENTS = 240;
/** Destroyed cover rises again this long after it falls, once no tank stands in the way. */
export const REBUILD_SECONDS = 6;
/** Hull half-length plus margin that must be clear around a footprint before it rebuilds. */
const REBUILD_CLEARANCE = 2;
/** Settled debris stays while the yard is below this share of its fragment budget. The base
 * cleanup fades the most distant pieces above 80%, so held debris retires gracefully. */
const DEBRIS_ROOM_FRACTION = 0.75;
/** Remaining life held on lingering debris; within the base cleanup's fade candidates. */
const DEBRIS_HOLD_SECONDS = 2;

const SIZE = {
  cargo: { w: 2.8, d: 2.8, h: 2.4, hp: 80, color: 0xb47a49 },
  drum: { w: 1.2, d: 1.2, h: 1.7, hp: 30, color: 0xff5b24 },
  tree: { w: 2.6, d: 2.6, h: 5.8, hp: 80, color: 0x218f55 },
} satisfies Partial<Record<CoverKind, Omit<CoverDef, "kind" | "x" | "z">>>;
const TIMBER_BAY = 3.7;
const TIMBER_DEPTH = 0.9;

function superstressLayout(): CoverDef[] {
  const covers: CoverDef[] = [];
  // Every placement has a twin rotated half a turn, so both teams meet the same yard.
  const pair = (cover: CoverDef) => {
    covers.push(cover);
    if (cover.x !== 0 || cover.z !== 0) {
      covers.push({ ...cover, x: -cover.x, z: -cover.z });
    }
  };
  const place = (kind: keyof typeof SIZE, x: number, z: number) =>
    pair({ kind, x, z, ...SIZE[kind] });
  /** A centred run of independently breakable timber bays. */
  const timberRun = (x: number, z: number, bays: number, alongX: boolean) => {
    for (let i = 0; i < bays; i++) {
      const offset = (i - (bays - 1) / 2) * TIMBER_BAY;
      pair({
        kind: "timber",
        x: alongX ? x + offset : x,
        z: alongX ? z : z + offset,
        w: alongX ? TIMBER_BAY : TIMBER_DEPTH,
        d: alongX ? TIMBER_DEPTH : TIMBER_BAY,
        h: 2.8,
        hp: TIMBER_HEALTH,
        color: 0xa66f46,
      });
    }
  };
  // Placements use standard-arena coordinates, like the shared spawns and pickups, so the
  // whole yard follows SUPERSTRESS_SCALE. Object sizes and cluster spacing stay in metres.
  const at = (standard: number) => standard * SUPERSTRESS_SCALE;
  const put = (kind: keyof typeof SIZE, x: number, z: number) => place(kind, at(x), at(z));
  const fence = (x: number, z: number, bays: number, alongX: boolean) =>
    timberRun(at(x), at(z), bays, alongX);
  const crateBlock = (x: number, z: number) => {
    for (const dx of [-1, 1]) {
      for (const dz of [-1, 1]) {
        place("cargo", at(x) + (dx * SIZE.cargo.w) / 2, at(z) + (dz * SIZE.cargo.d) / 2);
      }
    }
  };
  const drumTrio = (x: number, z: number) => {
    place("drum", at(x) - 0.65, at(z) - 0.4);
    place("drum", at(x) + 0.65, at(z) - 0.4);
    place("drum", at(x), at(z) + 0.7);
  };

  // A hard square fence keeps every body and chain reaction inside the yard.
  const wall = { kind: "boundary", h: 2.2, hp: Infinity, color: 0x7b7162 } as const;
  pair({ ...wall, x: YARD + 0.5, z: 0, w: 1, d: YARD * 2 + 2 });
  pair({ ...wall, x: 0, z: YARD + 0.5, w: YARD * 2 + 2, d: 1 });

  // The laser pickup sits in a powder-keg plaza inside a ring of timber with open corners.
  put("drum", 10, 0);
  put("drum", 0, 10);
  fence(0, 17, 2, true);
  fence(17, 0, 2, false);

  // Crate stacks, drum trios and lone trees chain into each other across both diagonals.
  crateBlock(25, 29);
  crateBlock(-25, 22);
  drumTrio(11, 28);
  drumTrio(-11, 26);
  drumTrio(-15, 9);
  drumTrio(-27, 35);
  put("tree", 25, 10);
  put("tree", -25, 8);
  put("drum", 31, 13);
  put("drum", -31, 11);
  put("cargo", 16, 9);
  put("cargo", 20, 41.6);

  // Timber alleys guard the rapid-fire and repair pickups beside each end wall.
  fence(8, 47.4, 2, false);
  fence(-8, 47.4, 2, false);
  put("cargo", 18, 52);
  put("cargo", -18, 52);
  put("cargo", 12.6, 56.8);
  put("cargo", -12.6, 56.8);

  // Groves shade the end walls; stumps keep blocking tanks after the crowns fall.
  for (const x of [-33, -24, 24, 33]) {
    put("tree", x, 56.6);
  }

  // Timber stubs divide each team's spawn lanes into garages with a drum in each corner.
  for (const z of [11.5, 34.5]) {
    for (const side of [-1, 1]) {
      timberRun(-YARD + TIMBER_BAY / 2, at(side * z), 1, true);
      place("drum", -YARD + 0.8, at(side * z) + side * 1.3);
    }
  }
  return covers;
}

export const SUPERSTRESS_MAP: ArenaMap = {
  id: "superstress",
  name: "Scrap Yard",
  description: "Compact yard · 30 tanks · cover rebuilds and debris lingers",
  floor: "packed-dirt",
  outerFloor: "dry-grass",
  outerFloorExtent: ARENA * 2 + 20,
  scale: SUPERSTRESS_SCALE,
  layout: superstressLayout,
};

function footprintOccupied(simulation: Simulation, cover: Cover): boolean {
  const x = cover.motion?.originX ?? cover.x;
  const z = cover.motion?.originZ ?? cover.z;
  const w = cover.motion?.w ?? cover.w;
  const d = cover.motion?.d ?? cover.d;
  return simulation.tanks.some((tank) => {
    if (!tank.alive) {
      return false;
    }
    const p = tank.body.translation();
    return (
      Math.abs(p.x - x) < w / 2 + REBUILD_CLEARANCE && Math.abs(p.z - z) < d / 2 + REBUILD_CLEARANCE
    );
  });
}

// Keyed by cover, so a reset's fresh cover records start without a pending rebuild.
const fallenAt = new WeakMap<Cover, number>();

/** Bring each destroyed cover back after REBUILD_SECONDS, waiting while a tank is in the way. */
function rebuildCover(simulation: Simulation): void {
  for (const cover of simulation.covers) {
    if (cover.alive || !cover.destructible) {
      continue;
    }
    const fallen = fallenAt.get(cover);
    if (fallen === undefined) {
      fallenAt.set(cover, simulation.elapsed);
      continue;
    }
    if (simulation.elapsed - fallen < REBUILD_SECONDS || footprintOccupied(simulation, cover)) {
      continue;
    }
    fallenAt.delete(cover);
    simulation.restoreCover(cover);
    simulation.events.push({
      type: "impact",
      id: cover.id,
      x: cover.x,
      z: cover.z,
      coverKind: cover.kind,
      color: cover.color,
      height: cover.h,
    });
  }
}

/** Hold debris just before its fade while the budget has room. When destruction fills it, the
 * base cleanup fades the most distant settled pieces first. */
function lingerDebris(simulation: Simulation): void {
  if (simulation.fragments.length >= simulation.maxFragments * DEBRIS_ROOM_FRACTION) {
    return;
  }
  const hold = DEBRIS_CLEANUP_SECONDS + DEBRIS_HOLD_SECONDS;
  for (const fragment of simulation.fragments) {
    if (fragment.life > DEBRIS_CLEANUP_SECONDS && fragment.life < hold) {
      fragment.life = hold;
      // A later blast relaunches debris only until this deadline; keep it ahead of the hold.
      if (fragment.expiresAt !== undefined) {
        fragment.expiresAt = Math.max(fragment.expiresAt, simulation.elapsed + hold);
      }
    }
  }
}

/** Level rules; they draw no gameplay randomness, so seeded matches stay reproducible. */
export function superstressRules(simulation: Simulation): void {
  rebuildCover(simulation);
  lingerDebris(simulation);
}

export const SUPERSTRESS_SETUP = {
  ...STRESS_TEST_SETUP,
  customMap: SUPERSTRESS_MAP,
  maxFragments: SUPERSTRESS_MAX_FRAGMENTS,
  afterStep: superstressRules,
} satisfies SimulationSetup;
