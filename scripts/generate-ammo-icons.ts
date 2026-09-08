import { createCanvas } from "@napi-rs/canvas";
import { mkdir, writeFile } from "node:fs/promises";
import { AMMO_ORDER, isSpecialAmmo } from "../src/game/ammunition";
import { WEAPONS } from "../src/game/data";

// Original vector pictograms rasterized offline; the game only loads these PNGs.
const output = new URL("../public/textures/pickups/", import.meta.url);
await mkdir(output, { recursive: true });
for (const kind of AMMO_ORDER.filter(isSpecialAmmo)) {
  const canvas = createCanvas(256, 256), c = canvas.getContext("2d");
  const color = `#${WEAPONS[kind].color.toString(16).padStart(6, "0")}`;
  c.fillStyle = color; c.fillRect(0, 0, 256, 256);
  c.fillStyle = "#14283a"; c.fillRect(14, 14, 228, 228);
  c.fillStyle = color; c.fillRect(22, 195, 212, 38);
  const shell = (x: number, y: number, angle = 0, rocket = false) => {
    c.save(); c.translate(x, y); c.rotate(angle);
    c.fillStyle = "#fff";
    c.beginPath(); c.moveTo(0, -49); c.lineTo(17, -25);
    c.lineTo(17, 34); c.lineTo(-17, 34); c.lineTo(-17, -25); c.closePath(); c.fill();
    c.fillStyle = color; c.fillRect(-17, 15, 34, 9);
    c.fillStyle = "#fff"; c.fillRect(-20, 38, 40, 8);
    if (rocket) {
      c.beginPath(); c.moveTo(-17, 3); c.lineTo(-32, 38); c.lineTo(-17, 30);
      c.moveTo(17, 3); c.lineTo(32, 38); c.lineTo(17, 30); c.fill();
    }
    c.restore();
  };
  c.strokeStyle = color; c.lineWidth = 9; c.lineJoin = "round";
  if (kind === "spread") {
    shell(68, 115, -0.28); shell(128, 102); shell(188, 115, 0.28);
  } else if (kind === "rocket") {
    shell(128, 104, 0, true);
    c.beginPath(); c.moveTo(115, 157); c.lineTo(128, 179); c.lineTo(141, 157); c.stroke();
  } else if (kind === "ricochet") {
    c.beginPath(); c.moveTo(48, 161); c.lineTo(195, 110); c.lineTo(117, 72); c.stroke();
    c.beginPath(); c.moveTo(209, 64); c.lineTo(209, 163); c.stroke();
    shell(92, 68, -1.05);
  } else {
    c.fillStyle = color; c.fillRect(45, 113, 57, 12); c.fillRect(154, 113, 57, 12);
    shell(128, 107);
    c.fillStyle = "#fff";
    for (const [x, y] of [[85, 94], [163, 94], [83, 139], [168, 140]]) c.fillRect(x, y, 8, 8);
  }
  c.fillStyle = "#14283a"; c.font = "900 25px sans-serif"; c.textAlign = "center";
  c.fillText(WEAPONS[kind].label, 128, 223);
  await writeFile(new URL(`${kind}.png`, output), canvas.toBuffer("image/png"));
}
