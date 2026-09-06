import { createCanvas } from "@napi-rs/canvas";
import { mkdir, writeFile } from "node:fs/promises";

// Offline atlas: side wrap on the left, circular lid on the right.
const canvas = createCanvas(512, 256), c = canvas.getContext("2d");
let seed = 47;
const random = () => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296);
c.fillStyle = "#c94b25"; c.fillRect(0, 0, 512, 256);
for (let i = 0; i < 5000; i++) {
  c.fillStyle = i % 3 ? "#703a2425" : "#f6ad7430";
  c.fillRect(random() * 512, random() * 256, 1 + random() * 3, 1 + random() * 7);
}
for (const y of [18, 218]) {
  c.fillStyle = "#593e2c"; c.fillRect(0, y, 384, 7);
  c.fillStyle = "#e58450"; c.fillRect(0, y + 7, 384, 2);
}
// Three large flame placards remain legible from different viewing directions.
for (const x of [64, 192, 320]) {
  c.save(); c.translate(x, 124); c.rotate(Math.PI / 4);
  c.fillStyle = "#271f19"; c.fillRect(-30, -30, 60, 60);
  c.fillStyle = "#f8db8b"; c.fillRect(-26, -26, 52, 52); c.restore();
  c.fillStyle = "#38261c"; c.beginPath();
  c.moveTo(x, 95); c.bezierCurveTo(x+5,113,x+23,118,x+15,138);
  c.bezierCurveTo(x+8,152,x-17,146,x-17,132);
  c.bezierCurveTo(x-19,124,x-8,111,x-8,111);
  c.lineTo(x-6,126); c.bezierCurveTo(x+3,117,x-5,108,x,95); c.fill();
  c.fillStyle = "#f8db8b"; c.beginPath(); c.moveTo(x,126);
  c.quadraticCurveTo(x+12,142,x,143); c.quadraticCurveTo(x-9,138,x,126); c.fill();
}
c.fillStyle = "#a64226"; c.fillRect(384, 0, 128, 256);
for (const radius of [53, 57]) {
  c.strokeStyle = radius === 53 ? "#e07b49" : "#553e30"; c.lineWidth = 3;
  c.beginPath(); c.arc(448, 128, radius, 0, Math.PI*2); c.stroke();
}
for (let i = 0; i < 100; i++) {
  c.fillStyle = "#563c2f60";
  c.fillRect(395 + random()*104, 76 + random()*104, random()*5+1, 1);
}
const target = new URL("../public/textures/barrels/", import.meta.url);
await mkdir(target, { recursive: true });
await writeFile(new URL("painted-drum.png", target), canvas.toBuffer("image/png"));
