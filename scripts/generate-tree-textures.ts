import { encodeWebp } from "./encode-webp";
import { createCanvas } from "@napi-rs/canvas";
import { mkdir, writeFile } from "node:fs/promises";
import { Random } from "./asset-data";

// Small, deterministic surface patterns. Runtime loads only these saved 256px WebP images.
const output = new URL("../public/textures/trees/", import.meta.url);
await mkdir(output, { recursive: true });
for (const kind of ["birch", "rings"]) {
  const rng = new Random(1729),
    canvas = createCanvas(256, 256),
    c = canvas.getContext("2d");
  c.fillStyle = kind === "birch" ? "#e6e1d5" : "#dfc89d";
  c.fillRect(0, 0, 256, 256);
  if (kind === "birch") {
    for (let i = 0; i < 220; i++) {
      c.fillStyle = i % 4 ? "#706d63" : "#aaa391";
      c.globalAlpha = rng.range(0.3, 0.8);
      c.fillRect(rng.range(-12, 256), rng.range(0, 256), rng.range(3, 25), rng.range(1, 4));
    }
  } else {
    c.lineWidth = 1.8;
    for (let ring = 1; ring < 23; ring++) {
      c.strokeStyle = ring % 3 ? "#b89a6e" : "#9c7c55";
      c.beginPath();
      for (let i = 0; i <= 100; i++) {
        const a = (i / 100) * Math.PI * 2;
        const r = ring * 7 * (1 + Math.sin(a * 3) * 0.025) + Math.sin(a * 7) * 1.1;
        const x = 124 + Math.cos(a) * r,
          y = 130 + Math.sin(a) * r * 0.96;
        if (i) c.lineTo(x, y);
        else c.moveTo(x, y);
      }
      c.stroke();
    }
    c.strokeStyle = "#775a3e";
    c.lineWidth = 2;
    for (const a of [0.6, 2.8, 4.7]) {
      c.beginPath();
      c.moveTo(124 + Math.cos(a) * 117, 130 + Math.sin(a) * 117);
      c.lineTo(124 + Math.cos(a + 0.06) * 68, 130 + Math.sin(a + 0.06) * 68);
      c.stroke();
    }
  }
  await writeFile(new URL(`${kind}.webp`, output), encodeWebp(canvas));
}
