import { encodePixelsWebp } from "./encode-webp";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { Random } from "./asset-data";
import { fbm, lattice, smooth } from "./texture-noise";

// Offline only: the tank livery's paint scheme and weathering, a seamless tile that
// multiplies the vertex-colored team paint (`tank_surfaces.rs`). Every value is a
// linear multiplier, so one tile serves both teams: a hand-painted two-tone
// camouflage of large, crisp-edged disruptive shapes in a clearly darker shade of the
// team's own hue (with a slightly cooler cast) and a few lighter brush strokes, faded
// paint, rain streaks, chipped edges showing dark primer, and fine scratches. The
// shapes are sized to survive the overhead camera: a tank carries two or three across
// its hull. The base tone is `PAINT_TEXTURE_MEAN` in Rust, so the vertex paint shows
// as intended on it and worn highlights have headroom. One tile covers
// 1 / PAINT_UV_PER_METRE (Rust) of armor.
const SIZE = 1024;
const sources = new URL("../assets/texture-sources/tanks/", import.meta.url);

/** Linear multipliers of the camouflage tones. */
const BASE = [0.8, 0.8, 0.8];
const DARK = [0.29, 0.27, 0.31];
const LIGHT = [1.02, 0.99, 0.94];
/** Paint built up along a dark shape's brushed edge. */
const EDGE_BUILDUP = 0.8;
/** Shares of the tile the dark shapes and the light strokes cover. */
const DARK_COVER = 0.36;
const LIGHT_COVER = 0.09;
/** Exposed primer and rust where the paint chipped away. */
const PRIMER = [0.3, 0.24, 0.2];

const field = new Float32Array(SIZE * SIZE * 3);
const random = new Random(9127);
const warpX = lattice(31, true);
const warpY = lattice(32, true);
const camoDark = lattice(33, true);
const camoLight = lattice(34, true);
const mottle = lattice(35, true);
const wear = lattice(36, true);
const brush = lattice(37, true);

/** The value below which `share` of `values` lies. */
function quantile(values: Float32Array, share: number) {
  const sorted = Float32Array.from(values).sort();
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * share))];
}

// Shape fields: a few large lobes per tile, domain-warped into organic brushed-on
// shapes whose edges wobble with a fine brush jitter.
const darkField = new Float32Array(SIZE * SIZE);
const lightField = new Float32Array(SIZE * SIZE);
for (let y = 0; y < SIZE; y++)
  for (let x = 0; x < SIZE; x++) {
    const u = x / SIZE,
      v = y / SIZE;
    const wx = fbm(warpX, u * 3, v * 3, 3, 3) - 0.5;
    const wy = fbm(warpY, u * 3, v * 3, 3, 3) - 0.5;
    const cu = u + wx * 0.16,
      cv = v + wy * 0.16;
    const jitter = (fbm(brush, u * 64, v * 64, 64, 2) - 0.5) * 0.012;
    darkField[y * SIZE + x] = fbm(camoDark, cu * 4, cv * 4, 4, 4) + jitter;
    lightField[y * SIZE + x] = fbm(camoLight, cu * 7, cv * 7, 7, 4) + jitter;
  }
const darkAt = quantile(darkField, 1 - DARK_COVER);
// Light strokes sit outside the dark shapes, so their threshold counts the rest.
const lightAt = quantile(lightField, 1 - LIGHT_COVER / (1 - DARK_COVER));

/** Signed distance in pixels to a shape's edge (positive inside), from the field's
 * slope, so every edge is equally crisp however fast its noise turns. */
function edgeDistance(values: Float32Array, threshold: number, x: number, y: number) {
  const at = (px: number, py: number) =>
    values[(((py + SIZE) % SIZE) * SIZE + ((px + SIZE) % SIZE)) | 0];
  const gx = (at(x + 1, y) - at(x - 1, y)) / 2;
  const gy = (at(x, y + 1) - at(x, y - 1)) / 2;
  return (at(x, y) - threshold) / Math.max(1e-6, Math.hypot(gx, gy));
}

for (let y = 0; y < SIZE; y++)
  for (let x = 0; x < SIZE; x++) {
    const u = x / SIZE,
      v = y / SIZE;
    const i = y * SIZE + x;
    const d = edgeDistance(darkField, darkAt, x, y);
    const dark = smooth(-0.8, 0.8, d);
    // A thin rim of heavier paint just inside each dark shape's edge.
    const rim = smooth(0.8, 2, d) * smooth(6, 3, d);
    const light = smooth(-0.8, 0.8, edgeDistance(lightField, lightAt, x, y)) * smooth(-3, -6, d);
    const fade = 0.95 + 0.1 * fbm(mottle, u * 12, v * 12, 12, 4);
    const grain = 1 + (random.next() - 0.5) * 0.035;
    for (let c = 0; c < 3; c++) {
      let tone = BASE[c] + (DARK[c] - BASE[c]) * dark + (LIGHT[c] - BASE[c]) * light;
      tone *= 1 - (1 - EDGE_BUILDUP) * rim;
      field[i * 3 + c] = tone * fade * grain;
    }
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
// on vertical armor.
for (let s = 0; s < 220; s++) {
  const x0 = random.next() * SIZE,
    y0 = random.next() * SIZE;
  const length = 40 + random.next() ** 1.5 * 260;
  const width = 1 + random.next() * 2.5;
  const strength = 0.05 + random.next() * 0.12;
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
for (let placed = 0, tries = 0; placed < 240 && tries < 40000; tries++) {
  const x0 = random.next() * SIZE,
    y0 = random.next() * SIZE;
  const mask = fbm(wear, (x0 / SIZE) * 6, (y0 / SIZE) * 6, 6, 3);
  if (random.next() > smooth(0.42, 0.62, mask)) continue;
  placed++;
  const radius = random.next() < 0.08 ? 3.5 + random.next() * 4 : 0.7 + random.next() * 1.8;
  const lobes = [random.next() * 6.28, random.next() * 6.28];
  const reach = Math.ceil(radius * 1.5 + 2);
  for (let dy = -reach; dy <= reach; dy++)
    for (let dx = -reach; dx <= reach; dx++) {
      const angle = Math.atan2(dy, dx);
      const edge =
        radius * (1 + 0.16 * Math.sin(angle * 2 + lobes[0]) + 0.1 * Math.sin(angle * 5 + lobes[1]));
      const d = Math.hypot(dx, dy);
      if (d < edge) multiply(x0 + dx, y0 + dy, PRIMER, smooth(edge, edge - 0.8, d));
      else if (d < edge + 1.3) multiply(x0 + dx, y0 + dy, 1.14, smooth(edge + 1.3, edge, d));
    }
}

// Fine scratches: short bright strokes through the paint film.
for (let s = 0; s < 260; s++) {
  const x0 = random.next() * SIZE,
    y0 = random.next() * SIZE;
  const angle = random.next() * Math.PI;
  const length = 6 + random.next() ** 2 * 50;
  for (let t = 0; t < length; t += 0.5) {
    multiply(
      x0 + Math.cos(angle) * t,
      y0 + Math.sin(angle) * t,
      1.16,
      0.35 + 0.65 * (1 - t / length),
    );
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
