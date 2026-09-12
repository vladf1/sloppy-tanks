import { createCanvas } from "@napi-rs/canvas";
import { writeFile } from "node:fs/promises";
import { Random } from "../src/game/data";

// One reusable, transparent needle spray. Alpha testing keeps foliage in the opaque pass.
const size = 512;
const canvas = createCanvas(size, size);
const c = canvas.getContext("2d");
const rng = new Random(73819);
const greens = ["#557344", "#728958", "#91a570", "#b0bc8a", "#c4cda0"];
function stroke(x: number, y: number, tx: number, ty: number, color: string, width: number) {
  c.strokeStyle = color;
  c.lineWidth = width;
  c.beginPath();
  c.moveTo(x, y);
  c.lineTo(tx, ty);
  c.stroke();
}
c.lineCap = "round";
stroke(256, 505, 257, 24, "#897757", 5);
for (let row = 0; row < 11; row++) {
  const t = row / 11;
  for (const side of [-1, 1]) {
    const x = 256 + rng.range(-5, 5);
    const y = 468 - t * 370 + rng.range(-8, 8);
    const reach = (Math.sin((t * 0.8 + 0.12) * Math.PI) * 145 + 24) * rng.range(0.85, 1.1);
    const tx = x + side * reach;
    const ty = y - rng.range(50, 95);
    stroke(x, y, tx, ty, "#6e7650", 3.2);
    // Needle bundles grow along lateral twigs, with brighter tips over shaded interiors.
    for (let i = 0; i < 28; i++) {
      const u = i / 28;
      const px = x + (tx - x) * u;
      const py = y + (ty - y) * u;
      for (const direction of [-1, 1]) {
        const angle = Math.atan2(ty - y, tx - x) + direction * rng.range(0.45, 1.05);
        const length = rng.range(19, 38) * (1 - u * 0.28);
        stroke(
          px,
          py,
          px + Math.cos(angle) * length,
          py + Math.sin(angle) * length,
          greens[Math.floor(rng.next() * greens.length)],
          rng.range(1.8, 3.2),
        );
      }
    }
  }
}
for (let i = 0; i < 75; i++) {
  const y = 26 + i * 5.8;
  for (const side of [-1, 1]) {
    stroke(257, y, 257 + side * rng.range(12, 27), y - rng.range(12, 30), greens[i % 5], 2.3);
  }
}
await writeFile(
  new URL("../public/textures/trees/conifer-spray.png", import.meta.url),
  canvas.toBuffer("image/png"),
);
