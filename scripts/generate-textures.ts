import { createCanvas, type Canvas } from "@napi-rs/canvas";
import { mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { PICKUPS } from "../src/game/data";
import "./generate-barrels";
import type { PickupKind } from "../src/game/types";

// Offline only: checked-in PNGs are loaded by the game, never this generator.
const output = new URL("../public/textures/", import.meta.url);
async function save(path: string, canvas: Canvas) {
  const target = new URL(path, output);
  await mkdir(new URL(".", target), { recursive: true });
  await writeFile(target, canvas.toBuffer("image/png"));
  console.log(fileURLToPath(target));
}

const names: Record<PickupKind, string> = {
  rapid: "RAPID", spread: "SPREAD", rocket: "ROCKET", ricochet: "BOUNCE",
  shield: "SHIELD", speed: "SPEED", repair: "REPAIR",
};

for (const kind of Object.keys(PICKUPS) as PickupKind[]) {
  const canvas = createCanvas(256, 256);
  const c = canvas.getContext("2d");
  const color = `#${PICKUPS[kind].color.toString(16).padStart(6, "0")}`;
  c.fillStyle = "#122638"; c.fillRect(0, 0, 256, 256);
  c.strokeStyle = color; c.lineWidth = 14; c.strokeRect(12, 12, 232, 232);
  c.fillStyle = color;
  for (const x of [28, 214]) for (const y of [28, 214]) c.fillRect(x, y, 14, 14);
  c.strokeStyle = c.fillStyle = "#ffffff";
  c.lineWidth = 16; c.lineCap = "round"; c.lineJoin = "round";
  const line = (points: number[][], fill = false) => {
    c.beginPath(); points.forEach(([x, y], i) => i ? c.lineTo(x, y) : c.moveTo(x, y));
    if (fill) { c.closePath(); c.fill(); } else c.stroke();
  };
  if (kind === "speed") line([[143, 46], [77, 126], [118, 126], [105, 181], [178, 96], [136, 96]], true);
  else if (kind === "repair") {
    c.fillRect(109, 57, 38, 124); c.fillRect(66, 100, 124, 38);
  } else if (kind === "shield") {
    line([[128, 52], [181, 73], [177, 126], [160, 157], [128, 183], [96, 157], [79, 126], [75, 73], [128, 52]]);
    line([[128, 78], [128, 152]]);
  } else if (kind === "rapid") {
    for (const x of [64, 105, 146]) line([[x, 73], [x + 35, 118], [x, 163]]);
  } else if (kind === "spread") {
    for (const x of [66, 128, 190]) {
      line([[128, 172], [x, 70]]);
      line([[x - 17, 82], [x, 58], [x + 17, 82]]);
    }
  } else if (kind === "ricochet") {
    c.strokeStyle = color; line([[190, 61], [190, 173]]);
    c.strokeStyle = "#fff";
    line([[67, 165], [166, 115], [77, 65]]);
    line([[82, 96], [67, 60], [107, 58]]);
  } else {
    line([[128, 47], [153, 79], [153, 140], [103, 140], [103, 79]], true);
    line([[103, 116], [81, 155], [105, 149]], true);
    line([[153, 116], [175, 155], [151, 149]], true);
    c.strokeStyle = color; line([[128, 155], [128, 183]]);
    c.fillStyle = "#122638"; c.beginPath(); c.arc(128, 94, 10, 0, Math.PI * 2); c.fill();
  }
  c.fillStyle = color; c.font = "900 25px sans-serif";
  c.textAlign = "center"; c.fillText(names[kind], 128, 218);
  await save(`pickups/${kind}.png`, canvas);
}

for (const kind of ["siding", "shingles"] as const) {
    const size = 256, pixels = new Uint8Array(size * size * 4);
    for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
      const row = Math.floor(y / 32), offset = y % 32;
      const width = kind === "shingles" ? 64 : 128;
      const column = Math.floor((x + (row % 2) * width / 2) / width);
      const seam = (x + (row % 2) * width / 2) % width;
      const variation = ((column * 37 + row * 19) % 23) - 11;
      const grain = kind === "siding"
        ? Math.sin(x * 0.12 + Math.sin(y * 0.7) * 2) * 4
        : ((x * 13 + y * 23) % 9) - 4;
      let value = 225 + variation + grain;
      if (offset < 3) value = 125;
      else if (offset < 5) value = 250;
      else if (offset > 28) value -= 24;
      if (seam < 2) value -= kind === "shingles" ? 65 : 25;
      const i = (y * size + x) * 4;
      pixels[i] = pixels[i + 1] = pixels[i + 2] = Math.max(0, Math.min(255, value));
      pixels[i + 3] = 255;
    }
  const canvas = createCanvas(size, size), c = canvas.getContext("2d");
  const image = c.createImageData(size, size);
  image.data.set(pixels);
  c.putImageData(image, 0, 0);
  await save(`houses/${kind}.png`, canvas);
}

for (const [team, symbol] of [["blue", "◆"], ["red", "Ⅱ"]]) {
  const canvas = createCanvas(256, 128), c = canvas.getContext("2d");
  c.fillStyle = "#ffffff";
  c.font = "bold 76px sans-serif";
  c.textAlign = "center";
  c.textBaseline = "middle";
  c.fillText(symbol, 128, 64);
  await save(`teams/${team}.png`, canvas);
}

// Neutral paint wear multiplies team paint without introducing another hue.
{
  const size = 512, canvas = createCanvas(size, size), c = canvas.getContext("2d");
  let state = 4817;
  const random = () => ((state = (Math.imul(state, 1664525) + 1013904223) >>> 0) / 4294967296);
  const image = c.createImageData(size, size);
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const broad = Math.sin(x / size * Math.PI * 6) * Math.cos(y / size * Math.PI * 4);
    const value = Math.min(255, Math.round(237 + broad * 15 + (random() - 0.5) * 12));
    const i = (y * size + x) * 4;
    image.data[i] = image.data[i + 1] = image.data[i + 2] = value;
    image.data[i + 3] = 255;
  }
  c.putImageData(image, 0, 0);
  // Broad rubbed paint survives mipmapping when a tank is only 40–80px wide.
  for (let i = 0; i < 28; i++) {
    const x = random() * size, y = random() * size;
    const radius = 18 + random() * 52;
    for (const ox of [-size, 0, size]) for (const oy of [-size, 0, size]) {
      const wash = c.createRadialGradient(x + ox, y + oy, 0, x + ox, y + oy, radius);
      wash.addColorStop(0, "#77777765"); wash.addColorStop(1, "#77777700");
      c.fillStyle = wash; c.fillRect(x + ox - radius, y + oy - radius, radius * 2, radius * 2);
    }
  }
  // Recessed plate joins and bolts give the paint readable manufactured detail.
  c.lineJoin = "round";
  for (const [x, y, w, h] of [[20, 22, 220, 198], [258, 22, 234, 198], [20, 238, 472, 254]]) {
    c.strokeStyle = "#929292"; c.lineWidth = 4;
    c.strokeRect(x, y, w, h);
    c.strokeStyle = "#ffffff"; c.lineWidth = 2;
    c.strokeRect(x + 3, y + 3, w - 6, h - 6);
    for (const bx of [x + 13, x + w - 13]) for (const by of [y + 13, y + h - 13]) {
      c.fillStyle = "#8a8a8a"; c.beginPath(); c.arc(bx, by, 4, 0, Math.PI * 2); c.fill();
      c.fillStyle = "#ffffff"; c.beginPath(); c.arc(bx - 1, by - 1, 2, 0, Math.PI * 2); c.fill();
    }
  }
  // Mix fine scratches with larger chipped streaks, wrapping across tile edges.
  for (let i = 0; i < 100; i++) {
    const x = random() * size, y = random() * size;
    const angle = random() * Math.PI * 2, length = 5 + random() ** 2 * 70;
    const dx = Math.cos(angle) * length, dy = Math.sin(angle) * length;
    const width = i < 24 ? 3 + random() * 3 : 0.8 + random() * 1.4;
    for (const ox of [-size, 0, size]) for (const oy of [-size, 0, size]) {
      c.lineWidth = width; c.strokeStyle = i < 24 ? "#aaaaaa" : "#bcbcbc";
      c.beginPath(); c.moveTo(x + ox, y + oy); c.lineTo(x + dx + ox, y + dy + oy); c.stroke();
      c.lineWidth = Math.max(1, width * 0.4); c.strokeStyle = "#ffffff";
      c.beginPath(); c.moveTo(x + ox, y + oy + width / 2); c.lineTo(x + dx + ox, y + dy + oy + width / 2); c.stroke();
    }
  }
  await save("tanks/armor-wear.png", canvas);
}
