import { createCanvas } from "@napi-rs/canvas";
import { writeFile } from "node:fs/promises";
import { Random } from "./asset-data";
import { encodePixelsWebp } from "./encode-webp";

// Broadleaf crowns are built from alpha-tested sprig cards (`tree_models.rs`
// `OAK_CELLS`, `OVATE_CELLS`): a 2 × 2 atlas, lobed oak sprigs on the top row and small ovate
// birch/aspen sprigs on the bottom row, each cell a twig rising from the bottom
// centre into a rounded mass of leaves. Leaves behind are painted darker so a
// card reads with depth; the crown's own shading comes from the mesh normals.
const size = 1024;
const cell = size / 2;
const canvas = createCanvas(size, size);
const ctx = canvas.getContext("2d");
const rng = new Random(40213);

// Light to dark sap greens; the material color tints them per family.
const greens = [
  [92, 120, 58],
  [108, 138, 64],
  [126, 152, 72],
  [146, 168, 84],
  [164, 182, 96],
  [118, 140, 82],
];

function shade([r, g, b]: number[], light: number) {
  const k = (v: number) => Math.max(0, Math.min(255, Math.round(v * light)));
  return `rgb(${k(r)}, ${k(g)}, ${k(b)})`;
}

/** Half-width of a leaf at `t` along its length, as a fraction of its length. */
function oakWidth(t: number, lobes: number, depth: number) {
  const body = Math.pow(Math.sin(Math.PI * Math.min(1, t * 1.08)), 0.85) * 0.34;
  // Rounded lobes between narrow sinuses.
  const lobe = 1 - depth * Math.pow(1 - Math.abs(Math.sin(t * lobes * Math.PI)), 2.5);
  return body * (t < 0.12 ? t / 0.12 : lobe);
}

function ovateWidth(t: number, teeth: number) {
  const body = Math.pow(Math.sin(Math.PI * Math.pow(t, 0.8)), 1.1) * 0.36;
  const serration = 1 - 0.06 * Math.abs(Math.sin(t * teeth * Math.PI));
  return body * serration;
}

/** One leaf from its stalk tip `(x, y)` pointing along `angle`. */
function leaf(x: number, y: number, angle: number, length: number, oak: boolean, light: number) {
  const color = greens[Math.floor(rng.next() * greens.length)];
  const lobes = 3 + Math.floor(rng.next() * 2);
  const depth = rng.range(0.45, 0.6);
  const teeth = 9 + Math.floor(rng.next() * 6);
  const bend = rng.range(-0.18, 0.18);
  const steps = 28;
  const point = (t: number, side: number) => {
    const w = (oak ? oakWidth(t, lobes, depth) : ovateWidth(t, teeth)) * length;
    const along = t * length;
    // The blade curls slightly sideways along its length.
    const curl = bend * along * t;
    const lx = along;
    const ly = side * w + curl;
    return [
      x + Math.cos(angle) * lx - Math.sin(angle) * ly,
      y + Math.sin(angle) * lx + Math.cos(angle) * ly,
    ];
  };
  ctx.beginPath();
  for (let i = 0; i <= steps; i++) {
    const [px, py] = point(i / steps, 1);
    if (i) ctx.lineTo(px, py);
    else ctx.moveTo(px, py);
  }
  for (let i = steps; i >= 0; i--) {
    const [px, py] = point(i / steps, -1);
    ctx.lineTo(px, py);
  }
  ctx.closePath();
  // One side of the midrib catches more light than the other.
  const [ax, ay] = point(0.5, 1);
  const [bx, by] = point(0.5, -1);
  const gradient = ctx.createLinearGradient(ax, ay, bx, by);
  gradient.addColorStop(0, shade(color, light * 1.12));
  gradient.addColorStop(1, shade(color, light * 0.82));
  ctx.fillStyle = gradient;
  ctx.fill();
  ctx.strokeStyle = shade(color, light * 0.62);
  ctx.lineWidth = 1.2;
  ctx.stroke();
  // Midrib and a few veins.
  ctx.strokeStyle = shade(color, light * 1.3);
  ctx.lineWidth = oak ? 1.6 : 1.1;
  ctx.beginPath();
  const [sx, sy] = point(0, 0);
  ctx.moveTo(sx, sy);
  for (let i = 1; i <= 10; i++) {
    const [px, py] = point((i / 10) * 0.94, 0);
    ctx.lineTo(px, py);
  }
  ctx.stroke();
  ctx.lineWidth = 0.8;
  for (let i = 1; i < (oak ? lobes * 2 : 6); i++) {
    const t = i / (oak ? lobes * 2 : 6);
    for (const side of [-1, 1]) {
      const [mx, my] = point(t * 0.9, 0);
      const [ex, ey] = point(Math.min(1, t * 0.9 + 0.12), side * 0.75);
      ctx.beginPath();
      ctx.moveTo(mx, my);
      ctx.lineTo(ex, ey);
      ctx.stroke();
    }
  }
}

function stroke(from: number[], to: number[], width: number) {
  ctx.lineWidth = width;
  ctx.beginPath();
  ctx.moveTo(from[0], from[1]);
  ctx.lineTo(to[0], to[1]);
  ctx.stroke();
}

/** A sprig in the cell at `(left, top)`: forking twigs carrying leaves. */
function sprig(left: number, top: number, oak: boolean) {
  ctx.save();
  ctx.beginPath();
  ctx.rect(left, top, cell, cell);
  ctx.clip();
  ctx.lineCap = "round";
  const base = [left + cell / 2, top + cell * 0.99];
  const fork = [left + cell / 2 + rng.range(-12, 12), top + cell * 0.74];
  type Leaf = { x: number; y: number; angle: number; length: number; depth: number };
  const leaves: Leaf[] = [];
  const twigs: number[][][] = [[base, fork]];
  const branches = oak ? 5 : 6;
  for (let i = 0; i < branches; i++) {
    const spread = (i / (branches - 1) - 0.5) * 2.3 + rng.range(-0.15, 0.15);
    const angle = -Math.PI / 2 + spread;
    const reach = cell * rng.range(0.38, 0.5) * (1 - Math.abs(spread) * 0.12);
    const start = [
      fork[0] + rng.range(-10, 10),
      fork[1] + rng.range(-6, 30) + Math.abs(spread) * 30,
    ];
    const end = [start[0] + Math.cos(angle) * reach, start[1] + Math.sin(angle) * reach];
    twigs.push([start, end]);
    const count = oak ? 10 : 17;
    for (let j = 0; j < count; j++) {
      const u = 0.2 + (j / count) * 0.85;
      const px = start[0] + (end[0] - start[0]) * u;
      const py = start[1] + (end[1] - start[1]) * u;
      const side = j % 2 ? 1 : -1;
      const tip = j === count - 1;
      leaves.push({
        x: px,
        y: py,
        angle: angle + (tip ? rng.range(-0.2, 0.2) : side * rng.range(0.55, 1.1)),
        length: (oak ? rng.range(62, 92) : rng.range(40, 58)) * (1 - u * 0.15),
        depth: rng.next(),
      });
    }
  }
  // Leaves near the fork fill the lower middle of the mass.
  for (let j = 0; j < (oak ? 6 : 12); j++) {
    leaves.push({
      x: fork[0] + rng.range(-30, 30),
      y: fork[1] + rng.range(-20, 40),
      angle: -Math.PI / 2 + rng.range(-1.6, 1.6),
      length: oak ? rng.range(60, 80) : rng.range(38, 52),
      depth: rng.next() * 0.5,
    });
  }
  ctx.strokeStyle = "#5d4c38";
  for (const [from, to] of twigs) stroke(from, to, from === base ? 7 : 3.5);
  // Paint back to front: leaves behind the sprig are darker.
  leaves.sort((a, b) => a.depth - b.depth);
  for (const l of leaves) {
    leaf(l.x, l.y, l.angle, l.length, oak, 0.68 + l.depth * 0.42);
  }
  ctx.restore();
}

for (const [column, row] of [
  [0, 0],
  [1, 0],
  [0, 1],
  [1, 1],
]) {
  sprig(column * cell, row * cell, row === 0);
}

// Transparent texels keep a leaf green, so mipmaps average toward foliage instead of
// darkening the cut-out edges. Canvas pixels are premultiplied, so the bled colour
// reaches cwebp as a raw RGBA PAM (lossless, `-exact`), not through the canvas.
const { data } = ctx.getImageData(0, 0, size, size);
for (let i = 0; i < data.length; i += 4) {
  if (data[i + 3] === 0) {
    data[i] = 88;
    data[i + 1] = 112;
    data[i + 2] = 56;
  }
}
await writeFile(
  new URL("../assets/texture-sources/trees/leaf-sprigs.webp", import.meta.url),
  encodePixelsWebp(data, size, size),
);
