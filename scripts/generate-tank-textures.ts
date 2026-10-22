import { encodePixelsWebp } from "./encode-webp";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { Random } from "./asset-data";
import { fbm, lattice, smooth } from "./texture-noise";

// Offline only: the tank livery's paint scheme and weathering, a seamless tile that
// multiplies the vertex-colored team paint (`tank_surfaces.rs`). Every value is a
// linear multiplier, so one tile serves both teams: a disruptive two-tone camouflage
// in darker and lighter shades of the team's own hue, faded and mottled paint, rain
// streaks, chipped edges showing dark primer, and fine scratches. The paint's mean
// sits below white (`PAINT_TEXTURE_MEAN` in Rust) so worn highlights have headroom.
// One tile covers 1 / PAINT_UV_PER_METRE (Rust) of armor.
const SIZE = 1024;
const sources = new URL("../assets/texture-sources/tanks/", import.meta.url);

/** Linear multipliers of the camouflage tones. */
const BASE = [0.78, 0.78, 0.78];
const DARK = [0.42, 0.42, 0.47];
const LIGHT = [0.97, 0.95, 0.9];
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

for (let y = 0; y < SIZE; y++)
  for (let x = 0; x < SIZE; x++) {
    const u = x / SIZE,
      v = y / SIZE;
    // A periodic domain warp turns the noise blobs into organic, brushed-on shapes.
    const wx = fbm(warpX, u * 3, v * 3, 3, 3) - 0.5;
    const wy = fbm(warpY, u * 3, v * 3, 3, 3) - 0.5;
    const cu = u + wx * 0.1,
      cv = v + wy * 0.1;
    const dark = smooth(0.536, 0.541, fbm(camoDark, cu * 6, cv * 6, 6, 4));
    const light = smooth(0.546, 0.551, fbm(camoLight, cu * 6, cv * 6, 6, 4)) * (1 - dark);
    const fade = 0.93 + 0.13 * fbm(mottle, u * 12, v * 12, 12, 4);
    const grain = 1 + (random.next() - 0.5) * 0.04;
    const i = (y * SIZE + x) * 3;
    for (let c = 0; c < 3; c++) {
      const tone = BASE[c] + (DARK[c] - BASE[c]) * dark + (LIGHT[c] - BASE[c]) * light;
      field[i + c] = tone * fade * grain;
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
