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
  rapid: 0xffb02a,
  shield: 0x4f94ff,
  speed: 0xf2f53a,
  repair: 0x2edb6e,
  laser: 0xe6f6ff,
};

export type PickupKind = SpecialAmmo | "rapid" | "shield" | "speed" | "repair" | "laser";

// The pickup atlas: one painted tile per kind in a 3x3 block; the pallet's bare
// planks and swatches of steel, rubber and webbing in the fourth column; cartridge
// brass below the block. The engine maps UVs with the same layout
// (`pickup_visuals.rs`).
export const PICKUP_ATLAS_PATH = "textures/pickups/atlas.webp";
export const PICKUP_ICON_SIZE = 256;
export const PICKUP_ATLAS_PADDING = 16;
export const PICKUP_ATLAS_STRIDE = PICKUP_ICON_SIZE + PICKUP_ATLAS_PADDING * 2;
export const PICKUP_ATLAS_SIZE = PICKUP_ATLAS_STRIDE * 4;
export const PICKUP_PALLET_CELL = [3, 0] as const;
export const PICKUP_SWATCH_CELL = [3, 2] as const;
export const PICKUP_BRASS_CELL = [0, 3] as const;
/** The surface each kind's supply shows under its paint: pine crates, olive drab
 * tubes, black ammo cans, jerry cans, black fibre shell canisters, rolled armour
 * steel, a ribbed gunmetal tool chest and a ribbed olive drab radio case. */
export const PICKUP_SURFACES = {
  spread: "planks",
  rocket: "olive",
  ricochet: "fibre",
  piercing: "planks",
  rapid: "can",
  shield: "steel",
  speed: "jerry",
  repair: "chest",
  laser: "case",
} as const satisfies Record<
  PickupKind,
  "planks" | "olive" | "fibre" | "can" | "steel" | "jerry" | "chest" | "case"
>;
/** Each kind's paint (`pickup_paint` in `pickup_visuals.rs`): the big painted
 * block on top of its load. Nine hues with none repeated (raspberry, burnt orange,
 * violet, teal, lime, pale steel blue, fuel yellow, green and off-white), as worn
 * enamel; the raspberry leans to purple and the steel blue is pale and grey, so
 * neither reads as a team's red or blue. */
export const PICKUP_PAINTS = {
  spread: 0xb44c88,
  rocket: 0xcc602c,
  ricochet: 0x7c5cbe,
  piercing: 0x229a90,
  rapid: 0x9eb63a,
  shield: 0x7aa4c6,
  speed: 0xe2b830,
  repair: 0x3c9a4e,
  laser: 0xe6e2d6,
} as const satisfies Record<PickupKind, number>;
/** The lot marking stencilled over and under each kind's emblem. */
export const PICKUP_MARKINGS = {
  spread: ["12 GA", "FLECHETTE"],
  rocket: ["RKT HE", "66 MM"],
  ricochet: ["105 MM", "HESH"],
  piercing: ["120 MM", "APFSDS"],
  rapid: ["CTG 7.62", "LINKED"],
  shield: ["ARMOR", "PLATE"],
  speed: ["FUEL", "20 L"],
  repair: ["TOOL", "KIT"],
  laser: ["APS", "RADIO SET"],
} as const satisfies Record<PickupKind, readonly [string, string]>;
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
