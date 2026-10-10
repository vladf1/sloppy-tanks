import { AMMO_OPTIONS } from "../src/game/ammo-options.ts";

// The few game values the offline asset generators paint with: pickup accent colors,
// the pickup atlas layout, and the seeded stream that keeps generated textures stable;
// ammunition labels and colors come from the HUD's `ammo-options.ts`. The game reads
// the same values from the engine (`crates/core/src/sim/data.rs`,
// `crates/core/src/models/pickup_visuals.rs`); change both together, then regenerate
// the assets.

/** Mulberry32, as the engine's `Random`: a fixed seed reproduces a texture exactly. */
export class Random {
  constructor(public state: number) {}
  next(): number {
    let t = (this.state += 0x6d2b79f5);
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }
  range(a: number, b: number): number {
    return a + (b - a) * this.next();
  }
}

/** The limited ammunition types, in the ammo bar's order after Standard. */
export const [, ...SPECIAL_AMMO] = AMMO_OPTIONS;
export type SpecialAmmo = (typeof SPECIAL_AMMO)[number]["weapon"];

/** Accent colors of the pickups that are not ammunition. */
export const PICKUP_COLORS = {
  rapid: 0xffcf54,
  shield: 0x72dbef,
  speed: 0xbbe574,
  repair: 0x88ddb0,
  laser: 0x7bfff2,
};

export type PickupKind = SpecialAmmo | "rapid" | "shield" | "speed" | "repair" | "laser";

// The pickup atlas: source pixels stay intact; the engine maps UVs with the same layout.
export const PICKUP_ATLAS_PATH = "textures/pickups/atlas.webp";
export const PICKUP_ICON_SIZE = 256;
export const PICKUP_ATLAS_PADDING = 16;
export const PICKUP_ATLAS_STRIDE = PICKUP_ICON_SIZE + PICKUP_ATLAS_PADDING * 2;
export const PICKUP_ATLAS_SIZE = PICKUP_ATLAS_STRIDE * 3;
export const PICKUP_ATLAS_TILES = {
  spread: [0, 0],
  rocket: [1, 0],
  ricochet: [2, 0],
  piercing: [0, 1],
  rapid: [1, 1],
  shield: [2, 1],
  speed: [0, 2],
  repair: [1, 2],
  laser: [2, 2],
} as const satisfies Record<PickupKind, readonly [number, number]>;
