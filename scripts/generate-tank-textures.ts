import { encodePixelsWebp } from "./encode-webp";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { Random } from "./asset-data";
import { fbm, lattice, smooth } from "./texture-noise";

// Offline only: the tank livery's paint finish, a seamless tile that multiplies the
// vertex-colored team paint (`tank_surfaces.rs`). Every value is a linear multiplier,
// so one tile serves both teams. It is a clean factory finish, not a camouflage: the
// team color must read at a glance from the overhead camera, so there are no large
// shapes in a second tone. What it adds is what a painted steel plate shows close up:
// a hand-brushed coat whose value drifts a little across the plate, brush strokes,
// a faint orange-peel grain, soft rain streaks, chips clustered where crews and
// branches knock the armor (dark primer with a bright rim of lifted paint) and fine
// bright scratches. The material's bump map reads the same tile, so chips and
// strokes catch the light. The base tone is `PAINT_TEXTURE_MEAN` in Rust, so the
// vertex paint shows as intended on it and worn highlights have headroom. One tile
// covers 1 / (PAINT_UV_PER_METRE * PAINT_REPEAT) metres of armor (Rust).
const SIZE = 1024;
const sources = new URL("../assets/texture-sources/tanks/", import.meta.url);

/** Linear multiplier of the clean coat. */
const BASE = 0.8;
/** How far the coat's value drifts across a plate, its brush strokes and grain. */
const DRIFT = 0.07;
const STROKES = 0.035;
const GRAIN = 0.03;
/** Exposed primer and rust where the paint chipped away. */
const PRIMER = [0.34, 0.27, 0.22];
/** Lifted paint round a chip and a fresh scratch: brighter than the coat. */
const CHIP_RIM = 1.16;
const SCRATCH = 1.22;

const field = new Float32Array(SIZE * SIZE * 3);
const random = new Random(9127);
const drift = lattice(35, true);
const hue = lattice(38, true);
const wear = lattice(36, true);
const brush = lattice(37, true);
const peel = lattice(39, true);

for (let y = 0; y < SIZE; y++)
  for (let x = 0; x < SIZE; x++) {
    const u = x / SIZE,
      v = y / SIZE;
    const i = y * SIZE + x;
    // A slow drift of the coat's value, as one plate's paint dried a shade lighter
    // than the next, with a slight warm-cool cast that keeps it from looking printed.
    const tone = 1 + DRIFT * (fbm(drift, u * 3, v * 3, 3, 4) - 0.5) * 2;
    const cast = (fbm(hue, u * 2, v * 2, 2, 3) - 0.5) * 0.04;
    // Brush strokes: noise stretched along u (across vertical plates, along the hull
    // top), broken up so no stroke runs the whole tile.
    const stroke =
      (fbm(brush, u * 6, v * 64, 6, 3) - 0.5) * 2 * (0.5 + fbm(brush, u * 4 + 7, v * 4, 4, 2));
    const orangePeel = fbm(peel, u * 160, v * 160, 160, 2) - 0.5;
    const grain = (random.next() - 0.5) * GRAIN;
    const value = BASE * tone * (1 + STROKES * stroke + 0.025 * orangePeel + grain);
    field[i * 3] = value * (1 + cast);
    field[i * 3 + 1] = value;
    field[i * 3 + 2] = value * (1 - cast);
  }

const wrap = (n: number) => ((n % SIZE) + SIZE) % SIZE;
const multiply = (x: number, y: number, factor: number[] | number, amount = 1) => {
  const i = (wrap(Math.round(y)) * SIZE + wrap(Math.round(x))) * 3;
  for (let c = 0; c < 3; c++) {
    const f = typeof factor === "number" ? factor : factor[c];
    field[i + c] *= 1 + (f - 1) * amount;
  }
};

// Rain and grime streaks run along the texture's v axis, which the livery maps to up
// on vertical armor: few and faint, so they read as weathering, not dirt.
for (let s = 0; s < 90; s++) {
  const x0 = random.next() * SIZE,
    y0 = random.next() * SIZE;
  const length = 40 + random.next() ** 1.5 * 220;
  const width = 1 + random.next() * 2;
  const strength = 0.03 + random.next() * 0.06;
  for (let t = 0; t < length; t++) {
    const along = Math.min(1, t / 10) * (1 - t / length) ** 0.7;
    const sway = Math.sin(t * 0.05 + s) * 0.8;
    for (let dx = -Math.ceil(width); dx <= Math.ceil(width); dx++) {
      const across = Math.max(0, 1 - Math.abs(dx) / width);
      multiply(x0 + dx + sway, y0 + t, 1 - strength, along * across);
    }
  }
}

// Chips cluster where the wear mask is high, like the knocks on exposed corners.
for (let placed = 0, tries = 0; placed < 170 && tries < 40000; tries++) {
  const x0 = random.next() * SIZE,
    y0 = random.next() * SIZE;
  const mask = fbm(wear, (x0 / SIZE) * 5, (y0 / SIZE) * 5, 5, 3);
  if (random.next() > smooth(0.45, 0.62, mask)) continue;
  placed++;
  const radius = random.next() < 0.1 ? 3 + random.next() * 3.5 : 1 + random.next() * 1.8;
  const lobes = [random.next() * 6.28, random.next() * 6.28];
  const reach = Math.ceil(radius * 1.5 + 3);
  for (let dy = -reach; dy <= reach; dy++)
    for (let dx = -reach; dx <= reach; dx++) {
      const angle = Math.atan2(dy, dx);
      const edge =
        radius * (1 + 0.18 * Math.sin(angle * 2 + lobes[0]) + 0.1 * Math.sin(angle * 5 + lobes[1]));
      const d = Math.hypot(dx, dy);
      if (d < edge) multiply(x0 + dx, y0 + dy, PRIMER, smooth(edge, edge - 0.8, d));
      else if (d < edge + 1.6) multiply(x0 + dx, y0 + dy, CHIP_RIM, smooth(edge + 1.6, edge, d));
    }
}

// Fine scratches: short bright strokes through the paint film, in loose bundles
// like a branch or a boot scraped them.
for (let bundle = 0; bundle < 70; bundle++) {
  const bx = random.next() * SIZE,
    by = random.next() * SIZE;
  const angle = random.next() * Math.PI;
  const strokes = 1 + Math.floor(random.next() * 4);
  for (let s = 0; s < strokes; s++) {
    const x0 = bx + (random.next() - 0.5) * 14,
      y0 = by + (random.next() - 0.5) * 14;
    const turn = angle + (random.next() - 0.5) * 0.25;
    const length = 8 + random.next() ** 2 * 60;
    for (let t = 0; t < length; t += 0.5) {
      multiply(
        x0 + Math.cos(turn) * t,
        y0 + Math.sin(turn) * t,
        SCRATCH,
        0.3 + 0.7 * Math.sin((Math.PI * t) / length),
      );
    }
  }
}

const srgb = (linear: number) =>
  linear <= 0.0031308 ? linear * 12.92 : 1.055 * linear ** (1 / 2.4) - 0.055;
const pixels = new Uint8ClampedArray(SIZE * SIZE * 4);
for (let p = 0; p < SIZE * SIZE; p++) {
  for (let c = 0; c < 3; c++) {
    pixels[p * 4 + c] = Math.round(255 * srgb(Math.min(1, Math.max(0, field[p * 3 + c]))));
  }
  pixels[p * 4 + 3] = 255;
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
