import { ARENA } from "./data";
import type { CoverDef } from "./arena";

/** Rotationally balanced lanes; outer deployment strips and shared pickups stay clear. */
export function harborLayout(): CoverDef[] {
  const covers: CoverDef[] = [];
  const add = (
    kind: CoverDef["kind"],
    x: number,
    z: number,
    w: number,
    d: number,
    h: number,
    hp: number,
    color: number,
  ) => covers.push({ kind, x, z, w, d, h, hp, color });
  for (const side of [-1, 1]) {
    add("boundary", side * (ARENA + 0.5), 0, 1, ARENA * 2 + 2, 1.2, Infinity, 0x879698);
    add("boundary", 0, side * (ARENA + 0.5), ARENA * 2 + 2, 1, 1.2, Infinity, 0x879698);
    for (const z of [-32, -12, 12, 32]) {
      add("container", side * 30, z, 6, 14, 3.6, Infinity, z < 0 ? 0xd37c38 : 0x31958d);
    }
    for (const z of [-8, 8]) {
      add("container", side * 13, z, 12, 5, 3.6, Infinity, 0x6689ad);
    }
    // Two individually breakable crates plug each shortcut between container rows.
    for (const z of [-22, 22]) {
      for (const x of [28.4, 31.6]) {
        add("cargo", side * x, z, 3.1, 3, 2.6, 90, 0xb88b53);
      }
      add("drum", side * 24, z, 1.2, 1.2, 1.7, 30, 0xff5b24);
    }
    for (const x of [10, 34]) {
      add("concrete", side * x, side * 55, 8, 1.1, 1.5, Infinity, 0xb5b5a5);
    }
    for (const x of [-4, 4]) {
      add("cargo", x, side * 20, 3, 3, 2.6, 90, 0xb88b53);
    }
    add("concrete", side * 43, 0, 1.2, 10, 1.7, Infinity, 0xb5b5a5);
  }
  return covers;
}
