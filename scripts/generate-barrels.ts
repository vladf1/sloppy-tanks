import { encodeLossyWebp } from "./encode-webp";
import { createCanvas } from "@napi-rs/canvas";
import { mkdir, writeFile } from "node:fs/promises";
import { fbm, lattice, mix, smooth } from "./texture-noise";

// Offline atlas for the explosive drum (`barrel_surfaces.rs`): the wall wrap on the
// left three quarters, the lid on the right. The wall runs from the top chime
// (row 0) to the bottom chime (row 255); its two rolling hoops sit at rows 38 and
// 218, where the lathed drum's ribs sample it. Red enamel over steel, worn bare
// on the hoops and chimes, scratched, rust-streaked from the top and grimy at the
// foot, with three flame placards that stay legible from any side. Encoded lossy
// (`DRUM_QUALITY`): the enamel is noise, which lossless packed at 86 KB.
const width = 512;
const height = 256;
const wall = 384;
const DRUM_QUALITY = 92;
const canvas = createCanvas(width, height);
const c = canvas.getContext("2d");
let seed = 47;
const random = () => (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296;
const hoops = [38, 218];

// Enamel and placards.
c.fillStyle = "#c4442a";
c.fillRect(0, 0, width, height);
for (const x of [64, 192, 320]) {
  c.save();
  c.translate(x, 128);
  c.rotate(Math.PI / 4);
  c.fillStyle = "#241c17";
  c.fillRect(-32, -32, 64, 64);
  c.fillStyle = "#f3d27e";
  c.fillRect(-27, -27, 54, 54);
  c.strokeStyle = "#241c17";
  c.lineWidth = 2;
  c.strokeRect(-22, -22, 44, 44);
  c.restore();
  c.fillStyle = "#2d1f17";
  c.beginPath();
  c.moveTo(x, 100);
  c.bezierCurveTo(x + 5, 116, x + 21, 121, x + 14, 139);
  c.bezierCurveTo(x + 8, 151, x - 16, 146, x - 16, 133);
  c.bezierCurveTo(x - 18, 125, x - 8, 113, x - 8, 113);
  c.lineTo(x - 6, 127);
  c.bezierCurveTo(x + 3, 118, x - 5, 110, x, 100);
  c.fill();
  c.fillStyle = "#f3d27e";
  c.beginPath();
  c.moveTo(x, 127);
  c.quadraticCurveTo(x + 11, 142, x, 143);
  c.quadraticCurveTo(x - 8, 138, x, 127);
  c.fill();
  // A stencilled bar under the placard.
  c.fillStyle = "#2a1d16";
  for (let k = -2; k <= 2; k++) c.fillRect(x - 3 + k * 9, 170, 6, 7);
}

// Pressed rings and bungs on the lid.
const lid = { x: 448, y: 128 };
for (const [radius, colour, line] of [
  [61, "#4c3a30", 4],
  [57, "#e2805a", 2],
  [44, "#a83a24", 3],
  [43, "#d9684a", 1],
  [26, "#a83a24", 2],
] as const) {
  c.strokeStyle = colour;
  c.lineWidth = line;
  c.beginPath();
  c.arc(lid.x, lid.y, radius, 0, Math.PI * 2);
  c.stroke();
}
for (const [bx, by, r] of [
  [lid.x + 32, lid.y, 10],
  [lid.x - 27, lid.y - 17, 7],
]) {
  c.fillStyle = "#3b2c24";
  c.beginPath();
  c.arc(bx, by, r + 3, 0, Math.PI * 2);
  c.fill();
}

// Per-pixel wear: mottled enamel, bare steel on the hoops and chimes, rust
// streaks running down from the top, grime toward the foot.
const image = c.getImageData(0, 0, width, height);
const data = image.data;
const mottle = lattice(11);
const chips = lattice(12);
const streak = lattice(13);
const STEEL = [118, 114, 108];
const RUST = [112, 58, 30];
const GRIME = [64, 46, 34];
for (let y = 0; y < height; y++) {
  for (let x = 0; x < width; x++) {
    const i = (y * width + x) * 4;
    let rgb = [data[i], data[i + 1], data[i + 2]];
    const onWall = x < wall;
    // The wall wraps horizontally every `wall` pixels.
    const u = onWall ? x / wall : (x - wall) / 128;
    const tone = fbm(mottle, u * 12, y / 18, onWall ? 12 : 0, 4);
    rgb = rgb.map((v) => v * (0.86 + tone * 0.28));
    let bare = 0;
    let rust = 0;
    let grime = 0;
    const chip = fbm(chips, u * 40, y / 6, onWall ? 40 : 0, 3);
    if (onWall) {
      const rim = Math.min(y, height - 1 - y);
      const hoop = Math.min(...hoops.map((h) => Math.abs(y - h)));
      // Raised steel takes the knocks: hoops and chimes wear through to metal.
      bare = Math.max(1 - smooth(2, 7, rim), 1 - smooth(2.5, 6, hoop)) * smooth(0.35, 0.55, chip);
      bare = Math.max(bare, smooth(0.66, 0.72, chip) * (1 - smooth(10, 24, hoop)) * 0.9);
      // Rust bleeds down from the chime and the upper hoop in thin runs.
      const run = streak(u * 70, 0, 70);
      const drip =
        smooth(0.6, 0.85, run) * (1 - smooth(0, 120, y)) * (0.6 + 0.4 * streak(u * 70, y / 30, 70));
      rust = Math.max(
        drip * 0.55,
        smooth(0.55, 0.8, chip) * (1 - smooth(1, 9, Math.min(rim, hoop))) * 0.6,
      );
      grime = smooth(170, 255, y) * (0.45 + tone * 0.4);
    } else {
      const r = Math.hypot(x - lid.x, y - lid.y);
      bare = (1 - smooth(1.5, 5, Math.abs(r - 61))) * smooth(0.4, 0.6, chip);
      rust = smooth(0.58, 0.8, chip) * smooth(48, 60, r) * 0.7;
      grime = smooth(30, 60, r) * 0.2 * tone;
    }
    rgb = rgb.map((v, k) => mix(v, STEEL[k] * (0.85 + chip * 0.4), bare));
    rgb = rgb.map((v, k) => mix(v, RUST[k], rust));
    rgb = rgb.map((v, k) => mix(v, GRIME[k], grime * 0.55));
    data[i] = rgb[0];
    data[i + 1] = rgb[1];
    data[i + 2] = rgb[2];
  }
}
c.putImageData(image, 0, 0);

// Scratches through the enamel: short bright strokes with a dark lip.
for (let n = 0; n < 80; n++) {
  const x = random() * wall;
  const y = 8 + random() * (height - 16);
  const angle = (random() - 0.5) * 1.2 + (random() < 0.5 ? 0 : Math.PI / 2);
  const length = 4 + random() * 18;
  const dx = Math.cos(angle) * length;
  const dy = Math.sin(angle) * length;
  c.strokeStyle = "#3a2a2280";
  c.lineWidth = 1.6;
  c.beginPath();
  c.moveTo(x, y + 0.8);
  c.lineTo(x + dx, y + dy + 0.8);
  c.stroke();
  c.strokeStyle = "#b9b2a6c0";
  c.lineWidth = 0.8;
  c.beginPath();
  c.moveTo(x, y);
  c.lineTo(x + dx, y + dy);
  c.stroke();
}

const target = new URL("../public/textures/barrels/", import.meta.url);
await mkdir(target, { recursive: true });
await writeFile(new URL("painted-drum.webp", target), encodeLossyWebp(canvas, DRUM_QUALITY));
