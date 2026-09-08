import { createCanvas } from "@napi-rs/canvas";
import { mkdir, writeFile } from "node:fs/promises";
import { PICKUPS } from "../src/game/data";

// Saved pictogram: a defensive emitter zaps an approaching shell.
const canvas = createCanvas(256, 256), c = canvas.getContext("2d");
const accent = `#${PICKUPS.laser.color.toString(16).padStart(6, "0")}`;
c.fillStyle = "#122638"; c.fillRect(0, 0, 256, 256);
c.strokeStyle = accent; c.lineWidth = 14; c.strokeRect(12, 12, 232, 232);
c.fillStyle = accent;
for (const x of [28, 214]) for (const y of [28, 214]) c.fillRect(x, y, 14, 14);
c.lineCap = "round"; c.lineJoin = "round";
c.strokeStyle = "white"; c.lineWidth = 10;
c.beginPath(); c.moveTo(64, 132); c.lineTo(64, 161); c.lineTo(96, 184); c.lineTo(128, 161); c.lineTo(128, 132); c.stroke();
c.fillStyle = accent; c.beginPath(); c.arc(96, 142, 13, 0, Math.PI * 2); c.fill();
c.lineWidth = 9; c.strokeStyle = accent;
c.beginPath(); c.moveTo(104, 132); c.lineTo(151, 94); c.stroke();
c.strokeStyle = "white"; c.lineWidth = 5;
c.beginPath(); c.moveTo(104, 132); c.lineTo(151, 94); c.stroke();
for (let i = 0; i < 6; i++) {
  const a = i * Math.PI / 3;
  c.beginPath(); c.moveTo(157 + Math.cos(a) * 9, 88 + Math.sin(a) * 9);
  c.lineTo(157 + Math.cos(a) * 23, 88 + Math.sin(a) * 23); c.stroke();
}
c.strokeStyle = accent; c.lineWidth = 13;
c.beginPath(); c.moveTo(180, 67); c.lineTo(193, 54); c.stroke();
c.font = "900 25px sans-serif"; c.textAlign = "center"; c.fillStyle = accent;
c.fillText("LASER", 128, 218);
const output = new URL("../public/textures/pickups/laser.png", import.meta.url);
await mkdir(new URL(".", output), { recursive: true });
await writeFile(output, canvas.toBuffer("image/png"));
console.log("Saved laser-defense pickup pictogram.");
