import type { CoverDef } from "./game/arena";
import { ARENA } from "./game/data";
import type { ArenaMap } from "./game/maps";
import type { Simulation } from "./game/simulation";
import type { CoverKind } from "./game/types";
import type { VehicleKind } from "./game/types";
import { TIMBER_HEALTH } from "./game/timber-layout";

export const STRESS_TANK_COUNT = 30;
export const STRESS_PLAYER_HEALTH_MULTIPLIER = 10_000;
export const STRESS_POWER_UP_MULTIPLIER = 10;
export const STRESS_AMMO_CRATE_MULTIPLIER = 10;
export const STRESS_PLAYER_KIND: VehicleKind = "scout";

function stressTestLayout(): CoverDef[] {
  const covers: CoverDef[] = [];
  const add = (
    kind: CoverKind,
    x: number,
    z: number,
    w: number,
    d: number,
    h: number,
    hp: number,
    color: number,
  ) => covers.push({ kind, x, z, w, d, h, hp, color });

  // A hard square perimeter keeps every body and chain reaction inside the test yard.
  for (const side of [-1, 1]) {
    add("boundary", side * (ARENA + 0.5), 0, 1, ARENA * 2 + 2, 2.2, Infinity, 0x7b7162);
    add("boundary", 0, side * (ARENA + 0.5), ARENA * 2 + 2, 1, 2.2, Infinity, 0x7b7162);
  }

  // Dense symmetric quadrants exercise draw calls, pathfinding, collisions and every major
  // destruction path while leaving the centre cross and team spawn strips driveable.
  const coordinates = [-42, -34, -26, -18, 18, 26, 34, 42];
  for (let xi = 0; xi < coordinates.length; xi++) {
    for (let zi = 0; zi < coordinates.length; zi++) {
      let x = coordinates[xi];
      let z = coordinates[zi];
      // Leave hull clearance around the shared rocket and ricochet pickup routes.
      if (Math.abs(x) === 18 && Math.abs(z) === 18) {
        x = Math.sign(x) * 20;
      } else if (Math.abs(x) === 18 && Math.abs(z) === 34) {
        z = Math.sign(z) * 32;
      }
      const pattern = (xi * 3 + zi * 5) % 7;
      if (Math.abs(x) === 26 && Math.abs(z) === 26) {
        add("tower", x, z, 6, 5, 7.5, 180, 0xbd864a);
      } else if (pattern === 0) {
        add("cargo", x, z, 2.8, 2.8, 2.4, 80, 0xb47a49);
      } else if (pattern === 1) {
        add("timber", x, z, xi % 2 ? 3.7 : 0.9, xi % 2 ? 0.9 : 3.7, 2.8, TIMBER_HEALTH, 0xa66f46);
      } else if (pattern === 2) {
        add("tree", x, z, 2.6, 2.6, 5.8, 80, 0x169f65);
      } else if (pattern === 3) {
        add("concrete", x, z, 3.2, 1.1, 2.2, Infinity, 0xb9b3a5);
      } else if (pattern === 4) {
        add("drum", x, z, 1.2, 1.2, 1.7, 30, 0xff5b24);
      } else if (pattern === 5) {
        add("teeth", x, z, 2.4, 2.4, 2.5, Infinity, 0xc8c2b5);
      } else {
        add("hedgehog", x, z, 2.9, 3.2, 2.7, Infinity, 0x5d6870);
      }
    }
  }

  // Breakable barricades create four temporary gates around the open centre.
  for (const side of [-1, 1]) {
    for (const offset of [-12, -6, 0, 6, 12]) {
      add("timber", offset, side * 22, 4.2, 0.9, 2.8, TIMBER_HEALTH, 0xb47a49);
      add("timber", side * 22, offset, 0.9, 4.2, 2.8, TIMBER_HEALTH, 0xb47a49);
    }
  }

  // Permanent buildings create hard sight-line breaks without sealing the broad central lanes.
  for (const side of [-1, 1]) {
    for (const offset of [-10, 10]) {
      add("house", offset, side * 35, 5.5, 6.5, 5, Infinity, 0xb87b4c);
      add("house", side * 35, offset, 6.5, 5.5, 5, Infinity, 0xc78b50);
    }
  }

  // Tree groves sit between the permanent houses, outer obstacle grid and spawn approaches.
  for (const side of [-1, 1]) {
    for (const offset of [-10, 10]) {
      add("tree", offset, side * 45, 2.8, 2.8, 6.2, 80, 0x169f65);
      add("tree", offset, side * 27, 2.6, 2.6, 5.8, 80, 0x218f55);
      add("tree", side * 45, offset, 2.8, 2.8, 6.2, 80, 0x169f65);
      add("tree", side * 27, offset, 2.6, 2.6, 5.8, 80, 0x218f55);
    }
  }

  // A final square belt alternates permanent concrete with heavy movable obstacles. It stays
  // between the combat field and deployment pads so the expanded roster can still spawn cleanly.
  const outerOffsets = [-36, -12, 12, 36];
  for (const side of [-1, 1]) {
    for (let i = 0; i < outerOffsets.length; i++) {
      const offset = outerOffsets[i];
      const kind: CoverKind = i % 3 === 0 ? "concrete" : i % 3 === 1 ? "teeth" : "hedgehog";
      const size =
        kind === "concrete"
          ? { w: 3.2, d: 1.1, h: 2.2, hp: Infinity, color: 0xb9b3a5 }
          : kind === "teeth"
            ? { w: 2.4, d: 2.4, h: 2.5, hp: Infinity, color: 0xc8c2b5 }
            : { w: 2.9, d: 3.2, h: 2.7, hp: Infinity, color: 0x5d6870 };
      add(kind, offset, side * 50, size.w, size.d, size.h, size.hp, size.color);
      add(kind, side * 50, offset, size.d, size.w, size.h, size.hp, size.color);
    }
  }

  // Extra permanent teeth guard the north and south verges without blocking spawn pads.
  for (const side of [-1, 1]) {
    for (const x of [-42, -28, -14, 14, 28, 42]) {
      add("teeth", x, side * 54, 2.4, 2.4, 2.5, Infinity, 0xc8c2b5);
    }
  }

  return covers;
}

export const STRESS_TEST_MAP: ArenaMap = {
  id: "stress-test",
  name: "Stress Grid",
  description: "30 tanks · 75 destructibles · permanent buildings and barriers",
  // Reuse only existing ground materials, without Pine Village's surrounding scenery.
  floor: "dry-grass",
  outerFloor: "packed-dirt",
  outerFloorExtent: ARENA * 2 + 20,
  layout: stressTestLayout,
};

export function configureStressTest(simulation: Simulation): void {
  simulation.customMap = STRESS_TEST_MAP;
  simulation.gameMode = "team";
  simulation.endlessMatch = true;
  simulation.humanKind = STRESS_PLAYER_KIND;
  simulation.roundCount = STRESS_TANK_COUNT;
  simulation.humanHealthMultiplier = STRESS_PLAYER_HEALTH_MULTIPLIER;
  simulation.powerUpDurationMultiplier = STRESS_POWER_UP_MULTIPLIER;
  simulation.ammoCrateMultiplier = STRESS_AMMO_CRATE_MULTIPLIER;
  simulation.reset(STRESS_TANK_COUNT);
}
