import { encodePixelsWebp } from "./encode-webp";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { Random } from "./asset-data";
import { fbm, lattice, smooth } from "./texture-noise";

// Offline only: the tank livery's paint finish, a seamless tile read by the vehicle
// paint shader (`tank_surfaces.rs`, `shaders/effects/vehicle_paint.wgsl`).
//
// RGB is the finish's value, a grey linear multiplier of the vertex-colored team
// paint, so one tile serves both teams: a hand-brushed coat whose value drifts a
// little across the plate, brush strokes, a faint orange-peel grain, soft rain
// streaks and fine bright scratches. The material's bump map reads the same tile,
// so strokes and scratches catch the light. The base tone is `PAINT_TEXTURE_MEAN`
// in Rust, so the vertex paint shows as intended on it.
//
// Alpha is the wear field: where the paint gives way first. The shader chips the
// paint to bare steel wherever the field falls under how exposed the spot is (plate
// edges most, the low hull more than the roof), so the same tile wears a clean roof
// lightly and a chamfer or a track guard hard. Most of the tile sits high; chips are
// pits in it, deepest at their centres, so more exposure grows each chip from its
// core outward; broad low patches scuff long stretches of the plate edges; scratch
// bundles cut thin grooves. It is stored losslessly (`-alpha_q 100`).
// One tile covers 1 / (PAINT_UV_PER_METRE * PAINT_REPEAT) metres of armor (Rust).
const SIZE = 1024;
const sources = new URL("../assets/texture-sources/tanks/", import.meta.url);

/** Linear multiplier of the clean coat. */
const BASE = 0.8;
/** How far the coat's value drifts across a plate, its brush strokes and grain. */
const DRIFT = 0.08;
const STROKES = 0.04;
const GRAIN = 0.025;
/** Fresh scratches: brighter than the coat. */
const SCRATCH = 1.25;
/** The wear field's range over clean paint: edges (exposure 0.5) scuff where it dips. */
const WEAR_LOW = 0.36;
const WEAR_HIGH = 0.98;
/** The most exposed spot (a chamfer low on the hull) chips below this. */
const WEAR_CEILING = 0.62;
const WEAR_STEPS = 64;

const field = new Float32Array(SIZE * SIZE);
const wearField = new Float32Array(SIZE * SIZE);
const random = new Random(9127);
const drift = lattice(35, true);
const wear = lattice(36, true);
const brush = lattice(37, true);
const peel = lattice(39, true);
const scuff = lattice(40, true);

for (let y = 0; y < SIZE; y++)
  for (let x = 0; x < SIZE; x++) {
    const u = x / SIZE,
      v = y / SIZE;
    const i = y * SIZE + x;
    // A slow drift of the coat's value, as one plate's paint dried a shade lighter
    // than the next.
    const tone = 1 + DRIFT * (fbm(drift, u * 3, v * 3, 3, 4) - 0.5) * 2;
    // Brush strokes: noise stretched along u (across vertical plates, along the hull
    // top), broken up so no stroke runs the whole tile.
    const stroke =
      (fbm(brush, u * 6, v * 64, 6, 3) - 0.5) * 2 * (0.5 + fbm(brush, u * 4 + 7, v * 4, 4, 2));
    const orangePeel = fbm(peel, u * 160, v * 160, 160, 2) - 0.5;
    const grain = (random.next() - 0.5) * GRAIN;
    field[i] = BASE * tone * (1 + STROKES * stroke + 0.025 * orangePeel + grain);
    // Broad scuffing: a fractal field stretched a little along u, sharpened so the
    // worn stretches of an edge have ragged ends.
    const scuffing = smooth(0.3, 0.68, fbm(scuff, u * 8, v * 5, 8, 5));
    wearField[i] = WEAR_LOW + (WEAR_HIGH - WEAR_LOW) * scuffing;
  }

const wrap = (n: number) => ((n % SIZE) + SIZE) % SIZE;
const at = (x: number, y: number) => wrap(Math.round(y)) * SIZE + wrap(Math.round(x));
const multiply = (x: number, y: number, factor: number, amount = 1) => {
  field[at(x, y)] *= 1 + (factor - 1) * amount;
};
const dig = (x: number, y: number, depth: number) => {
  const i = at(x, y);
  wearField[i] = Math.min(wearField[i], depth);
};

// Rain and grime streaks run along the texture's v axis, which the livery maps to up
// on vertical armor: few and faint, so they read as weathering, not dirt.
for (let s = 0; s < 150; s++) {
  const x0 = random.next() * SIZE,
    y0 = random.next() * SIZE;
  const length = 40 + random.next() ** 1.5 * 240;
  const width = 1 + random.next() * 2.5;
  const strength = 0.05 + random.next() * 0.11;
  for (let t = 0; t < length; t++) {
    const along = Math.min(1, t / 10) * (1 - t / length) ** 0.7;
    const sway = Math.sin(t * 0.05 + s) * 0.8;
    for (let dx = -Math.ceil(width); dx <= Math.ceil(width); dx++) {
      const across = Math.max(0, 1 - Math.abs(dx) / width);
      multiply(x0 + dx + sway, y0 + t, 1 - strength, along * across);
    }
  }
}

// Chips: irregular pits in the wear field, clustered where the wear mask is high
// like the knocks on exposed corners. A pit is deepest at its centre, so a spot
// worn harder shows the same chip larger.
for (let placed = 0, tries = 0; placed < 700 && tries < 80000; tries++) {
  const x0 = random.next() * SIZE,
    y0 = random.next() * SIZE;
  const mask = fbm(wear, (x0 / SIZE) * 5, (y0 / SIZE) * 5, 5, 3);
  if (random.next() > 0.15 + 0.85 * smooth(0.4, 0.6, mask)) continue;
  placed++;
  const radius = random.next() < 0.14 ? 9 + random.next() * 11 : 3 + random.next() * 5;
  const floor = 0.02 + random.next() * 0.2;
  const lobes = [random.next() * 6.28, random.next() * 6.28, random.next() * 6.28];
  const reach = Math.ceil(radius * 1.6 + 2);
  for (let dy = -reach; dy <= reach; dy++)
    for (let dx = -reach; dx <= reach; dx++) {
      const angle = Math.atan2(dy, dx);
      const edge =
        radius *
        (1 +
          0.22 * Math.sin(angle * 2 + lobes[0]) +
          0.12 * Math.sin(angle * 5 + lobes[1]) +
          0.06 * Math.sin(angle * 9 + lobes[2]));
      const d = Math.hypot(dx, dy) / edge;
      if (d < 1.25) dig(x0 + dx, y0 + dy, floor + (0.62 - floor) * d ** 1.4);
    }
}

// Fine scratches: short bright strokes through the paint film, in loose bundles
// like a branch or a boot scraped them; the deeper ones reach the steel.
for (let bundle = 0; bundle < 80; bundle++) {
  const bx = random.next() * SIZE,
    by = random.next() * SIZE;
  const angle = random.next() * Math.PI;
  const strokes = 1 + Math.floor(random.next() * 4);
  for (let s = 0; s < strokes; s++) {
    const x0 = bx + (random.next() - 0.5) * 14,
      y0 = by + (random.next() - 0.5) * 14;
    const turn = angle + (random.next() - 0.5) * 0.25;
    const length = 8 + random.next() ** 2 * 70;
    const depth = 0.12 + random.next() * 0.3;
    for (let t = 0; t < length; t += 0.5) {
      const along = Math.sin((Math.PI * t) / length);
      const px = x0 + Math.cos(turn) * t,
        py = y0 + Math.sin(turn) * t;
      multiply(px, py, SCRATCH, 0.3 + 0.7 * along);
      dig(px, py, depth + (1 - along) * 0.4);
    }
  }
}

const srgb = (linear: number) =>
  linear <= 0.0031308 ? linear * 12.92 : 1.055 * linear ** (1 / 2.4) - 0.055;
const pixels = new Uint8ClampedArray(SIZE * SIZE * 4);
for (let p = 0; p < SIZE * SIZE; p++) {
  const value = Math.round(255 * srgb(Math.min(1, Math.max(0, field[p]))));
  pixels[p * 4] = value;
  pixels[p * 4 + 1] = value;
  pixels[p * 4 + 2] = value;
  // Above `WEAR_CEILING` the field never shows (no spot is that exposed): flat
  // there, and in `WEAR_STEPS` levels, it compresses to a fraction of the size.
  const wearLevel = Math.round(Math.min(WEAR_CEILING, Math.max(0, wearField[p])) * WEAR_STEPS);
  pixels[p * 4 + 3] = Math.round((255 * wearLevel) / WEAR_STEPS);
}
await mkdir(sources, { recursive: true });
const target = new URL("armor-camo.webp", sources);
await writeFile(target, encodePixelsWebp(pixels, SIZE, SIZE));
console.log(fileURLToPath(target));

// The burnt-out wreck's paint (`wreck_model.rs`): a second tile on the same UVs that
// multiplies a pale ash base. Mottled soot over most of it, crazed grey ash where the
// paint burnt off entirely, rust-orange heat blooms, blistered flakes of paint and
// soot run down the plates. Linear multipliers, seamless, at half the camouflage's
// resolution: wrecks are seen from the gameplay cameras, not up close.
const SCORCH_SIZE = 512;
/** Linear multipliers of the burnt surfaces over the ash base. */
const SOOT = [0.2, 0.19, 0.18];
const ASH = [0.95, 0.93, 0.9];
const HEAT = [0.72, 0.4, 0.22];
const scorch = new Float32Array(SCORCH_SIZE * SCORCH_SIZE * 3);
const sootNoise = lattice(41, true);
const ashNoise = lattice(42, true);
const heatNoise = lattice(43, true);
const crackNoise = lattice(44, true);
const scorchRandom = new Random(4127);
for (let y = 0; y < SCORCH_SIZE; y++)
  for (let x = 0; x < SCORCH_SIZE; x++) {
    const u = x / SCORCH_SIZE,
      v = y / SCORCH_SIZE;
    const soot = fbm(sootNoise, u * 6, v * 6, 6, 5);
    // Ash lies where the fire burnt hottest, crazed by a fine crack network.
    const ash = smooth(0.56, 0.6, fbm(ashNoise, u * 4, v * 4, 4, 5));
    const crack = Math.abs(fbm(crackNoise, u * 24, v * 24, 24, 3) - 0.5);
    const crazed = ash * (1 - 0.7 * smooth(0.035, 0.0, crack));
    const heat = smooth(0.61, 0.68, fbm(heatNoise, u * 5, v * 5, 5, 4)) * (1 - ash);
    const shade = 0.75 + 0.5 * soot;
    const grain = 1 + (scorchRandom.next() - 0.5) * 0.12;
    const i = (y * SCORCH_SIZE + x) * 3;
    for (let c = 0; c < 3; c++) {
      let tone = SOOT[c] * shade;
      tone += (HEAT[c] * (0.55 + 0.4 * soot) - tone) * heat;
      tone += (ASH[c] * (0.7 + 0.3 * soot) - tone) * crazed;
      scorch[i + c] = tone * grain;
    }
  }
// Soot runs down the plates along v, like the camouflage tile's rain streaks.
for (let s = 0; s < 140; s++) {
  const x0 = scorchRandom.next() * SCORCH_SIZE,
    y0 = scorchRandom.next() * SCORCH_SIZE;
  const length = 20 + scorchRandom.next() ** 1.5 * 140;
  const width = 1 + scorchRandom.next() * 2;
  for (let t = 0; t < length; t++) {
    const along = (1 - t / length) ** 0.8;
    for (let dx = -Math.ceil(width); dx <= Math.ceil(width); dx++) {
      const across = Math.max(0, 1 - Math.abs(dx) / width);
      const px = (((Math.round(x0 + dx) % SCORCH_SIZE) + SCORCH_SIZE) % SCORCH_SIZE) | 0;
      const py = (((Math.round(y0 + t) % SCORCH_SIZE) + SCORCH_SIZE) % SCORCH_SIZE) | 0;
      const i = (py * SCORCH_SIZE + px) * 3;
      for (let c = 0; c < 3; c++) scorch[i + c] *= 1 - 0.45 * along * across;
    }
  }
}
// Blistered flakes: small pale lifted chips with a dark shadow edge.
for (let f = 0; f < 220; f++) {
  const x0 = scorchRandom.next() * SCORCH_SIZE,
    y0 = scorchRandom.next() * SCORCH_SIZE;
  const radius = 0.8 + scorchRandom.next() * 2.2;
  const reach = Math.ceil(radius + 2);
  for (let dy = -reach; dy <= reach; dy++)
    for (let dx = -reach; dx <= reach; dx++) {
      const d = Math.hypot(dx, dy);
      const px = (((Math.round(x0 + dx) % SCORCH_SIZE) + SCORCH_SIZE) % SCORCH_SIZE) | 0;
      const py = (((Math.round(y0 + dy) % SCORCH_SIZE) + SCORCH_SIZE) % SCORCH_SIZE) | 0;
      const i = (py * SCORCH_SIZE + px) * 3;
      const lift = smooth(radius, radius - 0.8, d);
      const rim = d > radius && d < radius + 1.2 && dy > 0 ? 0.6 : 1;
      for (let c = 0; c < 3; c++) {
        scorch[i + c] = (scorch[i + c] + (0.55 - scorch[i + c]) * lift * 0.7) * rim;
      }
    }
}
const scorchPixels = new Uint8ClampedArray(SCORCH_SIZE * SCORCH_SIZE * 4);
for (let p = 0; p < SCORCH_SIZE * SCORCH_SIZE; p++) {
  for (let c = 0; c < 3; c++) {
    scorchPixels[p * 4 + c] = Math.round(255 * srgb(Math.min(1, Math.max(0, scorch[p * 3 + c]))));
  }
  scorchPixels[p * 4 + 3] = 255;
}
const scorchTarget = new URL("armor-scorch.webp", sources);
await writeFile(scorchTarget, encodePixelsWebp(scorchPixels, SCORCH_SIZE, SCORCH_SIZE));
console.log(fileURLToPath(scorchTarget));
