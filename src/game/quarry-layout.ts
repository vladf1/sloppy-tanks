import { ARENA } from "./data";
import type { CoverDef } from "./arena";

/** Broad east/west crossing, sheltered outer loops and two crate-plugged rock cuts. */
export function quarryLayout(): CoverDef[] {
  const covers: CoverDef[] = [];
  const add = (
    kind: CoverDef["kind"],
    x: number,
    z: number,
    w: number,
    d: number,
    h: number,
    hp = Infinity,
    color = 0xd2bd99,
  ) => covers.push({ kind, x, z, w, d, h, hp, color });
  for (const side of [-1, 1]) {
    add("boundary", side * (ARENA + 0.5), 0, 1, ARENA * 2 + 2, 1.2);
    add("boundary", 0, side * (ARENA + 0.5), ARENA * 2 + 2, 1, 1.2);
    // Offset islands break cross-map fire without enclosing the central pickup.
    add("rock", side * 25, side * 12, 14, 8, 4.6);
    add("rock", side * 25, -side * 12, 14, 8, 3.8);
    add("rock", side * 4, side * 28, 16, 9, 4.8);
    add("rock", side * 40.5, side * 37, 10, 12, 4.2);
    add("rock", side * 24.5, side * 37, 10, 12, 3.6);
    add("rock", -side * 10, side * 47, 13, 7, 3.4);
    // Four pallet-sized supply crates form an orderly storage bay in each cut.
    // Their individual colliders leave visible seams and open progressively under fire.
    for (const x of [31.2, 33.8]) {
      for (const z of [35.4, 38.6]) {
        add("cargo", side * x, side * z, 2.4, 2.8, 2.1, 55, 0xa18e6f);
      }
    }
    // Small, separated cover islands leave the direct route wide enough to dodge.
    add("cargo", side * 9, -side * 7, 3, 3, 2.6, 90, 0xa18e6f);
    add("drum", side * 15, side * 27, 1.2, 1.2, 1.7, 30, 0xff5b24);
    // Crane-set, staggered ranks follow the verge with uneven gaps and offsets.
    // Mirror the same irregular belt for fair approaches; keep the haul road open.
    for (const [x, z, width, height] of [
      [41.7, -15.3, 1.9, 1.9],
      [42.25, -12.45, 2, 2.05],
      [41.85, -9.35, 1.9, 1.8],
      [42.5, -6.7, 1.9, 1.9],
      [45.25, -13.85, 2, 2.05],
      [44.7, -10.8, 1.9, 1.8],
      [45.4, -7.9, 1.9, 1.9],
      [45.05, -4.75, 2, 2.05],
    ]) {
      add("teeth", side * x, side * z, width, width, height);
    }
    // A close-set steel line ties into each midfield rock shoulder.
    for (let i = 0; i < 4; i++) {
      add("hedgehog", side * (15.8 + i * 2.4), side * 22, 2.32, 2.56, 2.16);
    }
  }
  return covers;
}
