import { ARENA } from "./data";

export const ROAD_SHOULDER = 0.7;
// Shared by the road meshes and surface effects so their boundaries stay aligned.
export const VILLAGE_ROADS = [
  ...[-52, 0, 52].map((x) => ({
    x,
    z: 0,
    w: x === 0 ? 18 : 10,
    d: ARENA * 2 - 2,
    y: 0.0425,
  })),
  ...[-38, 0, 38].map((z) => ({
    x: 0,
    z,
    w: ARENA * 2 - 2,
    d: z === 0 ? 12 : 8,
    y: 0.0625,
  })),
];

/** Dust starts on the solid dirt, leaving the grass-blended shoulders quiet. */
export function isVillageDirt(x: number, z: number): boolean {
  return VILLAGE_ROADS.some(
    (road) =>
      Math.abs(x - road.x) <= road.w / 2 - ROAD_SHOULDER &&
      Math.abs(z - road.z) <= road.d / 2 - ROAD_SHOULDER,
  );
}
