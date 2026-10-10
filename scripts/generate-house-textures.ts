import { mkdir, writeFile } from "node:fs/promises";
import { Random } from "./asset-data";
import { encodePixelsWebp } from "./encode-webp";
import { fbm, lattice, mix, smooth } from "./texture-noise";

// Cottage and watchtower surfaces (`building_kit.rs`, `house_model.rs`), drawn as
// lossless sources under assets/texture-sources/houses/ for optimize-textures.mjs.
// Every image tiles in both directions and doubles as its own bump map (red
// channel = height), so joints and shadowed laps are darker than the faces.
//
// - clapboard: 12 courses of painted lap siding, near white so the paint tints
//   it; the top of each course lies in the shadow of the butt above.
// - brick: 16 courses of running bond with dark recessed joints.
// - stone: mortared fieldstone, irregular rounded stones in dark joints.
// - shingles: 8 courses of asphalt shingles in the legacy row order
//   (`house_surfaces.rs`, unflipped): each row's top is a course's butt edge at
//   the eave side, keyways run up from it and the next course's butt shades the
//   row's bottom.
const size = 1024;
const folder = new URL("../assets/texture-sources/houses/", import.meta.url);

/** Tileable fractal noise of a point in [0, 1)², `cells` cells across a lattice that
 * wraps in both axes. */
const tileable = (
  noise: ReturnType<typeof lattice>,
  u: number,
  v: number,
  cells: number,
  octaves = 4,
) => fbm(noise, u * cells, v * cells, cells, octaves);

/** Distance from `x` to `mark`, wrapping at 1. */
const wrapDistance = (x: number, mark: number) => {
  const d = Math.abs(x - mark) % 1;
  return Math.min(d, 1 - d);
};

async function save(name: string, paint: (u: number, v: number) => number[]) {
  const data = new Uint8ClampedArray(size * size * 4);
  const sum = [0, 0, 0];
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const rgb = paint((x + 0.5) / size, (y + 0.5) / size);
      const i = (y * size + x) * 4;
      for (let c = 0; c < 3; c++) {
        data[i + c] = rgb[c];
        sum[c] += data[i + c];
      }
      data[i + 3] = 255;
    }
  }
  await mkdir(folder, { recursive: true });
  await writeFile(new URL(`${name}.webp`, folder), encodePixelsWebp(data, size, size));
  const average = sum.map((c) =>
    Math.round(c / (size * size))
      .toString(16)
      .padStart(2, "0"),
  );
  console.log(`${name}: average sRGB #${average.join("")}`);
}

// Clapboard: board butt joints fall at random places, a few per course.
{
  const courses = 12;
  const rng = new Random(5021);
  const joints = Array.from({ length: courses }, () =>
    Array.from({ length: 1 + Math.floor(rng.next() * 2) }, () => rng.next()),
  );
  const shades = Array.from({ length: courses * 3 }, () => rng.range(-0.025, 0.025));
  const grain = lattice(11, true);
  const wear = lattice(12, true);
  await save("clapboard", (u, v) => {
    const row = Math.floor(v * courses);
    const t = v * courses - row;
    // Shadow under the butt above, the board face catching more light lower down,
    // and the butt's own underside along the bottom.
    let shade = 0.93 + 0.07 * t;
    shade *= mix(0.5, 1, smooth(0.0, 0.16, t));
    shade *= 1 - 0.28 * smooth(0.955, 0.995, t);
    let board = 0;
    for (const joint of joints[row]) {
      const d = wrapDistance(u, joint) * size;
      shade *= mix(0.55, 1, smooth(0.6, 2.2, d));
      if (u > joint) board++;
    }
    shade *= 1 + shades[row * 3 + (board % 3)];
    // Grain telegraphing through the paint, and gentle weathering.
    shade *= 1 + (grain(u * 6, v * 220, 6) - 0.5) * 0.05;
    shade *= 1 + (tileable(wear, u, v, 5) - 0.5) * 0.08;
    const chip = tileable(wear, u + 0.37, v + 0.11, 24, 3);
    shade *= chip > 0.74 ? 0.86 : 1;
    return [236, 232, 224].map((c) => c * shade);
  });
}

// Brick: 5 bricks per course, half-brick offset on alternate courses.
{
  const courses = 16;
  const bricks = 5;
  const rng = new Random(7713);
  const tones = Array.from({ length: courses * bricks }, () => ({
    value: rng.range(0.82, 1.1),
    warm: rng.range(-0.06, 0.06),
    clinker: rng.next() < 0.08,
  }));
  const grit = lattice(21, true);
  const soot = lattice(22, true);
  const mortarU = 0.009;
  const mortarV = 0.0085;
  await save("brick", (u, v) => {
    const row = Math.floor(v * courses);
    const t = v * courses - row;
    const shifted = u * bricks + (row % 2) * 0.5;
    const column = Math.floor(shifted);
    const s = shifted - column;
    const edge = Math.min(Math.min(s, 1 - s) / bricks, Math.min(t, 1 - t) / courses);
    const joint = 1 - smooth(Math.min(mortarU, mortarV) * 0.5, Math.max(mortarU, mortarV), edge);
    const tone = tones[row * bricks + (((column % bricks) + bricks) % bricks)];
    const g = tileable(grit, u, v, 64, 3) - 0.5;
    let brick = [158, 74, 52].map(
      (c, i) => c * tone.value * (1 + (i === 0 ? tone.warm : -tone.warm)),
    );
    if (tone.clinker) brick = brick.map((c) => c * 0.74);
    // Rounded arrises and pitted faces.
    const arris = smooth(0, 0.004, edge);
    brick = brick.map((c) => c * (0.86 + 0.14 * arris) * (1 + g * 0.22));
    const mortar = [104, 99, 92].map((c) => c * (1 + g * 0.3));
    const sooty = 1 - 0.12 * smooth(0.55, 0.8, tileable(soot, u, v, 3));
    return brick.map((c, i) => mix(c, mortar[i], joint) * sooty);
  });
}

// Fieldstone: jittered cells on a wrapping grid, rounded stones in deep joints.
{
  const cells = 7;
  const rng = new Random(9137);
  const seeds = Array.from({ length: cells * cells }, (_, i) => ({
    x: ((i % cells) + rng.range(0.15, 0.85)) / cells,
    y: (Math.floor(i / cells) + rng.range(0.15, 0.85)) / cells,
    tone: rng.range(0.78, 1.12),
    hue: rng.next(),
  }));
  const grit = lattice(31, true);
  const lichen = lattice(32, true);
  await save("stone", (u, v) => {
    let first = Infinity;
    let second = Infinity;
    let nearest = seeds[0];
    const cx = Math.floor(u * cells);
    const cy = Math.floor(v * cells);
    for (let dy = -2; dy <= 2; dy++) {
      for (let dx = -2; dx <= 2; dx++) {
        const gx = cx + dx;
        const gy = cy + dy;
        const seed =
          seeds[(((gy % cells) + cells) % cells) * cells + (((gx % cells) + cells) % cells)];
        const sx = seed.x + Math.floor(gx / cells);
        const sy = seed.y + Math.floor(gy / cells);
        // Stones lie in courses: squash them a little vertically.
        const d = Math.hypot(u - sx, (v - sy) * 1.35);
        if (d < first) {
          second = first;
          first = d;
          nearest = seed;
        } else if (d < second) second = d;
      }
    }
    // Distance to the joint between the two nearest stones, wobbled.
    const wobble = (tileable(grit, u, v, 12, 3) - 0.5) * 0.012;
    const gap = (second - first) / 2 + wobble;
    const joint = 1 - smooth(0.004, 0.012, gap);
    const dome = smooth(0.004, 0.05, gap);
    const g = tileable(grit, u + 0.5, v, 48, 4) - 0.5;
    const colors = [
      [150, 140, 124],
      [128, 122, 114],
      [162, 146, 118],
      [118, 110, 100],
    ];
    const base = colors[Math.floor(nearest.hue * colors.length)];
    const moss = smooth(0.62, 0.75, tileable(lichen, u, v, 4)) * 0.35;
    const stone = base.map(
      (c, i) =>
        mix(c, [96, 112, 70][i], moss) * nearest.tone * (0.78 + 0.22 * dome) * (1 + g * 0.25),
    );
    const mortar = [78, 74, 68].map((c) => c * (1 + g * 0.4));
    return stone.map((c, i) => mix(c, mortar[i], joint));
  });
}

// Asphalt shingles: 6 tabs per course, keyways offset half a tab on alternate
// courses, granule speckle and blended tab tones.
{
  const courses = 8;
  const tabs = 6;
  const rng = new Random(4441);
  const tones = Array.from({ length: courses * tabs }, () => rng.range(0.86, 1.08));
  const granules = lattice(41, true);
  const streaks = lattice(42, true);
  await save("shingles", (u, v) => {
    const row = Math.floor(v * courses);
    const t = v * courses - row;
    const shifted = u * tabs + (row % 2) * 0.5;
    const tab = Math.floor(shifted);
    const s = shifted - tab;
    let shade = tones[row * tabs + (((tab % tabs) + tabs) % tabs)];
    // Butt edge: a dark sliver of the end face, then a bright lip.
    shade *= t < 0.025 ? 0.55 : 1 + 0.08 * (1 - smooth(0.025, 0.08, t));
    // The next course's butt shades the bottom of this row.
    shade *= mix(1, 0.58, smooth(0.78, 1, t));
    // Keyways between tabs run from the butt up over half the exposure.
    const slot = Math.min(s, 1 - s) * (size / tabs);
    if (t < 0.6) shade *= mix(0.42, 1, smooth(1.2, 3.5, slot));
    const speck = granules(u * 170, v * 170, 170);
    shade *= 0.92 + 0.16 * speck;
    shade *= 1 + (tileable(streaks, u, v, 4) - 0.5) * 0.12;
    return [214, 210, 204].map((c) => c * shade);
  });
}
