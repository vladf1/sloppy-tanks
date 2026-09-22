import type { PickupKind } from "./types";

// Shared by the offline packer and runtime UV mapping. Keep source pixels intact.
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

export function pickupAtlasUV(kind: PickupKind, u: number, v: number): [number, number] {
  const [column, row] = PICKUP_ATLAS_TILES[kind];
  return [
    (column * PICKUP_ATLAS_STRIDE + PICKUP_ATLAS_PADDING + u * PICKUP_ICON_SIZE) /
      PICKUP_ATLAS_SIZE,
    1 -
      (row * PICKUP_ATLAS_STRIDE + PICKUP_ATLAS_PADDING + (1 - v) * PICKUP_ICON_SIZE) /
        PICKUP_ATLAS_SIZE,
  ];
}
