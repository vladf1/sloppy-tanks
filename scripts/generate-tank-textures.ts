import { encodePixelsWebp } from "./encode-webp";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { Random } from "./asset-data";
import { fbm, lattice, smooth } from "./texture-noise";

// Offline only: the tank livery's wear tile, a seamless data tile read by the
// vehicle paint shader (`tank_surfaces.rs`, `shaders/effects/vehicle_paint.wgsl`),
// which paints the clean team color itself and only asks the tile where wear goes.
//
// Alpha is the chip field: high over sound paint, with ragged pits where the paint
// gives way first. The shader chips to bare steel wherever the field falls under how
// exposed a spot is: next to a convex plate edge most, the low hull a little, the
// open face of a plate almost never. So the same tile leaves upper panels clean and
// wears edges and corners in broken runs of chips. A pit is deepest at its centre, so
// a more exposed spot shows the same chip larger. Stored losslessly.
//
// The grey RGB is the grime field around 0.5: broad splashes that break the line of
// the dust the tracks throw up, and soft streaks running down v (up on upright
// plates) where rain carried dirt below bolts and seams. The shader uses it only in
// the dusty band and along recesses, never as a speckle over clean paint.
// One tile covers 1 / (PAINT_UV_PER_METRE * PAINT_REPEAT) metres of armor (Rust).
const SIZE = 512;
const sources = new URL("../assets/texture-sources/tanks/", import.meta.url);

/** The chip field over sound paint and the floor of a chip's pit. */
const SOUND_LOW = 0.45;
const SOUND_HIGH = 1;
const CHIPS = 900;

const grime = new Float32Array(SIZE * SIZE);
const chips = new Float32Array(SIZE * SIZE);
const random = new Random(9127);
const splash = lattice(35, true);
const scuff = lattice(36, true);
const cluster = lattice(37, true);
const ragged = lattice(40, true);

for (let y = 0; y < SIZE; y++)
  for (let x = 0; x < SIZE; x++) {
    const u = x / SIZE,
      v = y / SIZE;
    const i = y * SIZE + x;
    // Splashes: two scales of fractal noise, the coarse one stretched along u so the
    // dust line climbs and falls in long waves.
    grime[i] = 0.5 + 0.75 * (fbm(splash, u * 6, v * 9, 6, 4) - 0.5) +
      0.35 * (fbm(splash, u * 24 + 5, v * 24, 24, 3) - 0.5);
    // Broad scuffing along exposed edges: a fractal field, ragged at its fine end so a
    // worn run has broken ends rather than a soft fade.
    const scuffing = smooth(0.32, 0.7, fbm(scuff, u * 7, v * 7, 7, 5));
    const tear = fbm(ragged, u * 48, v * 48, 48, 3);
    chips[i] = SOUND_LOW + (SOUND_HIGH - SOUND_LOW) * scuffing * (0.82 + 0.36 * tear);
  }

const wrap = (n: number) => ((n % SIZE) + SIZE) % SIZE;
const at = (x: number, y: number) => wrap(Math.round(y)) * SIZE + wrap(Math.round(x));

// Rain streaks below seams: soft, darker runs down v.
for (let s = 0; s < 70; s++) {
  const x0 = random.next() * SIZE,
    y0 = random.next() * SIZE;
  const length = 30 + random.next() ** 1.5 * 140;
  const width = 1 + random.next() * 2.5;
  const strength = 0.08 + random.next() * 0.14;
  for (let t = 0; t < length; t++) {
    const along = Math.min(1, t / 6) * (1 - t / length) ** 0.7;
    for (let dx = -Math.ceil(width); dx <= Math.ceil(width); dx++) {
      const across = Math.max(0, 1 - Math.abs(dx) / width);
      grime[at(x0 + dx, y0 - t)] -= strength * along * across;
    }
  }
}

// Chips: irregular pits, clustered like the knocks along a corner, mostly small with
// a few big flakes. Each is deepest at its centre.
for (let placed = 0, tries = 0; placed < CHIPS && tries < 100000; tries++) {
  const x0 = random.next() * SIZE,
    y0 = random.next() * SIZE;
  const mask = fbm(cluster, (x0 / SIZE) * 6, (y0 / SIZE) * 6, 6, 3);
  if (random.next() > 0.2 + 0.8 * smooth(0.42, 0.62, mask)) continue;
  placed++;
  const radius = random.next() < 0.12 ? 5 + random.next() * 6 : 1.5 + random.next() * 3;
  const floor = random.next() * 0.12;
  const lobes = [random.next() * 6.28, random.next() * 6.28, random.next() * 6.28];
  const reach = Math.ceil(radius * 1.6 + 2);
  for (let dy = -reach; dy <= reach; dy++)
    for (let dx = -reach; dx <= reach; dx++) {
      const angle = Math.atan2(dy, dx);
      const edge =
        radius *
        (1 +
          0.25 * Math.sin(angle * 2 + lobes[0]) +
          0.14 * Math.sin(angle * 5 + lobes[1]) +
          0.07 * Math.sin(angle * 9 + lobes[2]));
      const d = Math.hypot(dx, dy) / edge;
      if (d < 1.4) {
        const i = at(x0 + dx, y0 + dy);
        chips[i] = Math.min(chips[i], floor + (0.75 - floor) * d ** 1.3);
      }
    }
}

const pixels = new Uint8ClampedArray(SIZE * SIZE * 4);
for (let p = 0; p < SIZE * SIZE; p++) {
  // Data, not color: the material samples the tile linearly.
  const value = Math.round(255 * Math.min(1, Math.max(0, grime[p])));
  pixels[p * 4] = value;
  pixels[p * 4 + 1] = value;
  pixels[p * 4 + 2] = value;
  pixels[p * 4 + 3] = Math.round(255 * Math.min(1, Math.max(0, chips[p])));
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
const srgb = (linear: number) =>
  linear <= 0.0031308 ? linear * 12.92 : 1.055 * linear ** (1 / 2.4) - 0.055;
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
