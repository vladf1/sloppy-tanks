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
    add("rock", side * 40, side * 37, 10, 12, 4.2);
    add("rock", side * 25, side * 37, 10, 12, 3.6);
    add("rock", -side * 10, side * 47, 13, 7, 3.4);
    // A 5m cut between each outer pair opens when its two crates are destroyed.
    for (const z of [34, 40]) {
      add("cargo", side * 32.5, side * z, 5, 5.8, 2.6, 90, 0xa18e6f);
    }
    // Small, separated cover islands leave the direct route wide enough to dodge.
    add("cargo", side * 9, -side * 7, 3, 3, 2.6, 90, 0xa18e6f);
    add("drum", side * 15, side * 27, 1.2, 1.2, 1.7, 30, 0xff5b24);
    // Short staggered rows protect deployment approaches; both ends stay open.
    for (let i = 0; i < 3; i++) {
      add("teeth", side * (43 + (i % 2) * 2.5), side * (-13 + i * 4), 1.9, 1.9, 1.9);
    }
    add("hedgehog", side * 15, side * 6, 2.9, 3.2, 2.7);
    add("hedgehog", -side * 19, side * 42, 2.9, 3.2, 2.7);
  }
  return covers;
}
