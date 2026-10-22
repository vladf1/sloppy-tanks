import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { Random } from "./asset-data";
import { fbm, lattice, mix, smooth } from "./texture-noise";

// Offline only: writes Dusty Dig's bastion wall atlas (`quarry_barriers.rs`), the
// sand-filled wire-mesh cells that line the arena. The left half is one cell's
// side: a desert-tan geotextile liner behind a welded square mesh, the spring-coil
// joints at both edges, sand splashed up from the foot, dust washed down from the
// rim, sun-bleaching and the odd tear showing the fill. The right half is the
// cell's top: the sand fill inside the mesh rim. Rows run from the cell's top
// (row 0) to its foot.
//
//   node --import tsx scripts/generate-quarry-textures.ts

const WIDTH = 512;
const HEIGHT = 256;
/** Pixels across one half: a cell is a metre wide, and its side 1.2 m tall. */
const HALF = 256;
const CELL_HEIGHT = 1.2;
/** Welded mesh pitch and wire radius, metres. */
const MESH = 0.0762;
const WIRE = 0.0022;
const output = fileURLToPath(new URL("../public/textures/quarry/", import.meta.url));

type Rgb = [number, number, number];
const LINER: Rgb = [0.68, 0.52, 0.31];
const SOIL: Rgb = [0.5, 0.37, 0.22];
const FILL: Rgb = [0.55, 0.42, 0.25];
const STEEL: Rgb = [0.4, 0.37, 0.31];
const RUST: Rgb = [0.3, 0.14, 0.06];

function encodeLossy(name: string, pixels: Uint8ClampedArray, quality: number) {
  const dir = mkdtempSync(join(tmpdir(), "quarry-textures-"));
  try {
    const pam = join(dir, "texture.pam");
    const header = `P7\nWIDTH ${WIDTH}\nHEIGHT ${HEIGHT}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n`;
    writeFileSync(pam, Buffer.concat([Buffer.from(header, "ascii"), Buffer.from(pixels.buffer)]));
    const target = join(output, name);
    execFileSync("cwebp", [
      "-quiet",
      "-q",
      String(quality),
      "-m",
      "6",
      "-metadata",
      "none",
      pam,
      "-o",
      target,
    ]);
    console.log(target);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** Coverage of a wire of radius `WIRE` at `offset` metres from its axis, given
 * the metres one pixel spans. */
const wire = (offset: number, pixel: number) =>
  smooth(WIRE + pixel * 0.7, WIRE - pixel * 0.3, Math.abs(offset));

/** Distance from `x` to the nearest multiple of `pitch`, signed. */
const toGrid = (x: number, pitch: number) => x - Math.round(x / pitch) * pitch;

const blend = (a: Rgb, b: Rgb, t: number): Rgb => [
  mix(a[0], b[0], t),
  mix(a[1], b[1], t),
  mix(a[2], b[2], t),
];
const scale = (a: Rgb, s: number): Rgb => [a[0] * s, a[1] * s, a[2] * s];

function bastionAtlas() {
  const random = new Random(4417);
  const weave = lattice(31, true);
  const mottle = lattice(32, true);
  const streaks = lattice(33, true);
  const rust = lattice(34, true);
  const grit = lattice(35, true);
  const pixels = new Float32Array(WIDTH * HEIGHT * 3);
  const put = (x: number, y: number, color: Rgb) => pixels.set(color, (y * WIDTH + x) * 3);
  const tears = Array.from({ length: 3 }, () => ({
    u: random.range(0.2, 0.8),
    v: random.range(0.25, 0.7),
    rx: random.range(0.02, 0.05),
    ry: random.range(0.03, 0.08),
  }));
  // The side: u across the cell (0..1 m), v up from its foot (0..1.2 m).
  const pixelU = 1 / HALF;
  const pixelV = CELL_HEIGHT / HEIGHT;
  for (let y = 0; y < HEIGHT; y++) {
    const v = (1 - (y + 0.5) / HEIGHT) * CELL_HEIGHT;
    for (let x = 0; x < HALF; x++) {
      const u = (x + 0.5) / HALF;
      // Non-woven liner: felted fibres over a slow mottle of sun and grime,
      // bleached toward the rim, which takes the sun all day.
      const fibre = fbm(weave, u * 96, v * 80, 96, 3);
      const patches = fbm(mottle, u * 4, v * 3.3, 4, 4);
      const shade =
        mix(0.9, 1.08, fibre) * mix(0.9, 1.08, patches) * mix(1.0, 1.08, smooth(0.6, 1.15, v));
      let color = scale(LINER, shade);
      // Dust washed down from the rim in ragged runs.
      const run = smooth(0.55, 0.85, fbm(streaks, u * 22, v * 1.5, 22, 3)) * smooth(0.2, 0.9, v);
      color = blend(color, scale(SOIL, 1.15), run * 0.28);
      // Sand and soil splashed up from the foot, its edge ragged.
      const splash = 0.32 + 0.12 * fbm(mottle, u * 9 + 5, 0.5, 9, 3);
      const foot = smooth(splash + 0.1, splash - 0.12, v);
      color = blend(color, SOIL, foot * 0.85);
      // Tears through the liner show the darker fill and a frayed pale lip.
      for (const tear of tears) {
        const r = Math.hypot((u - tear.u) / tear.rx, (v - tear.v) / tear.ry);
        const ragged = r + (fbm(grit, u * 60, v * 60, 60, 2) - 0.5) * 0.6;
        if (ragged < 1.25) {
          const hole = smooth(1.0, 0.8, ragged);
          const lip = smooth(1.25, 1.05, ragged) * (1 - hole);
          color = blend(blend(color, scale(color, 1.18), lip), scale(FILL, 0.8), hole);
        }
      }
      // The welded mesh in front of the liner: square wires with a lit upper
      // edge and a soft shadow cast down and right onto the liner.
      const du = toGrid(u, MESH);
      const dv = toGrid(v - 0.02, MESH);
      const shadow = Math.max(wire(du - 0.006, pixelU * 2), wire(dv + 0.006, pixelV * 2));
      color = scale(color, 1 - shadow * 0.22);
      const vertical = wire(du, pixelU);
      const horizontal = wire(dv, pixelV);
      const steel = Math.max(vertical, horizontal);
      if (steel > 0) {
        const rusty = Math.min(1, smooth(0.55, 0.8, fbm(rust, u * 14, v * 12, 14, 3)) + foot * 0.5);
        const lit =
          1 + 0.25 * Math.max(smooth(0, WIRE, -dv) * horizontal, smooth(0, WIRE, -du) * vertical);
        color = blend(color, scale(blend(STEEL, RUST, rusty), lit), steel * 0.75);
      }
      // The spring-coil joint at each edge: a dark band of turns.
      const edge = Math.min(u, 1 - u);
      if (edge < 0.016) {
        const turn = 0.5 + 0.5 * Math.sin(((v + u * 0.4) / 0.026) * Math.PI * 2);
        const coil = smooth(0.016, 0.01, edge) * mix(0.55, 1.0, turn);
        color = blend(color, scale(STEEL, mix(0.6, 1.1, turn)), coil);
      }
      // The liner's folded top edge and the rim wire.
      const rim = smooth(CELL_HEIGHT - 0.035, CELL_HEIGHT - 0.012, v);
      color = blend(color, scale(STEEL, 1.1), rim * 0.6);
      put(x, y, color);
    }
  }
  // The top: sand fill seen from above inside the rim, u and w across (0..1 m).
  for (let y = 0; y < HEIGHT; y++) {
    const w = (y + 0.5) / HEIGHT;
    for (let x = 0; x < HALF; x++) {
      const u = (x + 0.5) / HALF;
      const fine = fbm(grit, u * 70, w * 70, 70, 3);
      const broad = fbm(mottle, u * 3 + 11, w * 3, 3, 3);
      // Wind ripples across the fill and the odd pebble.
      const ripple = Math.sin((u * 0.8 + w * 0.6) * 70 + broad * 6) * 0.03;
      const pebble = fbm(weave, u * 30 + 3, w * 30, 30, 2);
      const shade =
        (mix(0.86, 1.1, fine) * mix(0.92, 1.06, broad) + ripple) *
        (pebble > 0.68 ? 1.12 : pebble < 0.3 ? 0.88 : 1);
      let color = scale(FILL, 1.2 * shade);
      // Fill settles away from the rim: the liner's top edge shows round it,
      // shading the sand at its foot, the rim wire on top.
      const edge = Math.min(u, 1 - u, w, 1 - w);
      const liner = smooth(0.03, 0.02, edge);
      const lee = smooth(0.06, 0.03, edge) * (1 - liner);
      color = scale(color, 1 - lee * 0.12);
      color = blend(color, scale(LINER, 1.08), liner);
      const rimWire = smooth(0.012, 0.004, Math.abs(edge - 0.008));
      color = blend(color, scale(STEEL, 1.2), rimWire * 0.8);
      put(HALF + x, y, color);
    }
  }
  const bytes = new Uint8ClampedArray(WIDTH * HEIGHT * 4);
  for (let p = 0; p < WIDTH * HEIGHT; p++) {
    for (let c = 0; c < 3; c++) {
      const linear = Math.min(1, Math.max(0, pixels[p * 3 + c]));
      bytes[p * 4 + c] = Math.round(255 * linear ** (1 / 2.2));
    }
    bytes[p * 4 + 3] = 255;
  }
  encodeLossy("bastion.webp", bytes, 88);
}

bastionAtlas();
