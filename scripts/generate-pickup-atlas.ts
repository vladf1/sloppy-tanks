import { createCanvas, loadImage, type Image } from "@napi-rs/canvas";
import { writeFile } from "node:fs/promises";
import { encodeWebp } from "./encode-webp";
import {
  PICKUP_ATLAS_PATH,
  PICKUP_ATLAS_PADDING,
  PICKUP_ATLAS_SIZE,
  PICKUP_ATLAS_STRIDE,
  PICKUP_ATLAS_TILES,
  PICKUP_ICON_SIZE,
} from "./asset-data";

// Pack the lossless pictogram artwork offline. Each icon's dark navy panel is lit
// in its kind's color (a glow behind the symbol fading to a deep tint at the frame)
// so a cube reads by color from the battle camera, not only by its thin frame. Then
// extrude edge pixels into every gutter so filtering/mipmaps do not sample the
// neighboring pickup's icon.

/** A pixel on the panel every icon is drawn on, and one on its kind-colored frame. */
const PANEL_SEED = [128, 40] as const;
const FRAME_SAMPLE = [10, 128] as const;
/** How far (RGB distance) a pixel may stray from the panel color and still fill. */
const PANEL_TOLERANCE = 24;
/** Anti-aliased panel edges: partial weight within this RGB distance of the panel. */
const PANEL_EDGE_DISTANCE = 220;
/** Panel lighting: the centre of the glow, its radius in pixels, and the share of
 * the kind color at the glow's heart and at the panel's edge. */
const GLOW_CENTER = [128, 116] as const;
const GLOW_RADIUS = 150;
const GLOW_MIX = 0.6;
const EDGE_MIX = 0.22;
/** A soft top-left sheen across the panel, as if lit from the sun's side. */
const SHEEN = 0.07;

function lightPanel(image: Image): ReturnType<typeof createCanvas> {
  const size = PICKUP_ICON_SIZE;
  const canvas = createCanvas(size, size);
  const context = canvas.getContext("2d");
  context.drawImage(image, 0, 0);
  const pixels = context.getImageData(0, 0, size, size);
  const data = pixels.data;
  const at = (x: number, y: number) => (y * size + x) * 4;
  const seed = at(...PANEL_SEED);
  const panel = [data[seed], data[seed + 1], data[seed + 2]];
  const frame = at(...FRAME_SAMPLE);
  const kind = [data[frame], data[frame + 1], data[frame + 2]];
  const distance = (i: number) =>
    Math.hypot(data[i] - panel[0], data[i + 1] - panel[1], data[i + 2] - panel[2]);
  // Flood the panel from the seed: labels drawn in panel color on a colored bar
  // are not connected to it and keep their contrast.
  const inside = new Uint8Array(size * size);
  const stack = [PANEL_SEED[1] * size + PANEL_SEED[0]];
  inside[stack[0]] = 1;
  while (stack.length) {
    const index = stack.pop()!;
    const x = index % size;
    const y = (index - x) / size;
    for (const [nx, ny] of [
      [x - 1, y],
      [x + 1, y],
      [x, y - 1],
      [x, y + 1],
    ]) {
      if (nx < 0 || ny < 0 || nx >= size || ny >= size) continue;
      const next = ny * size + nx;
      if (inside[next] || distance(next * 4) > PANEL_TOLERANCE) continue;
      inside[next] = 1;
      stack.push(next);
    }
  }
  const nearPanel = (x: number, y: number) => {
    for (let dy = -2; dy <= 2; dy++) {
      for (let dx = -2; dx <= 2; dx++) {
        const nx = x + dx;
        const ny = y + dy;
        if (nx >= 0 && ny >= 0 && nx < size && ny < size && inside[ny * size + nx]) return true;
      }
    }
    return false;
  };
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const i = at(x, y);
      const weight = inside[y * size + x]
        ? 1
        : nearPanel(x, y)
          ? Math.max(0, 1 - distance(i) / PANEL_EDGE_DISTANCE)
          : 0;
      if (weight === 0) continue;
      const r = Math.hypot(x - GLOW_CENTER[0], y - GLOW_CENTER[1]) / GLOW_RADIUS;
      const glow = Math.exp(-r * r * 2.2);
      const mix = EDGE_MIX + (GLOW_MIX - EDGE_MIX) * glow;
      const sheen = 1 + SHEEN * (1 - (x + y) / size);
      for (let c = 0; c < 3; c++) {
        const lit = (panel[c] + (kind[c] * 0.7 - panel[c]) * mix) * sheen;
        data[i + c] = Math.round(data[i + c] + (lit - panel[c]) * weight);
      }
    }
  }
  context.putImageData(pixels, 0, 0);
  return canvas;
}

const canvas = createCanvas(PICKUP_ATLAS_SIZE, PICKUP_ATLAS_SIZE);
const context = canvas.getContext("2d");
context.imageSmoothingEnabled = false;
for (const [kind, [column, row]] of Object.entries(PICKUP_ATLAS_TILES)) {
  const image = await loadImage(
    new URL(`../assets/texture-sources/pickups/${kind}.webp`, import.meta.url),
  );
  if (image.width !== PICKUP_ICON_SIZE || image.height !== PICKUP_ICON_SIZE) {
    throw new Error(`Unexpected pickup icon dimensions: ${kind}`);
  }
  const icon = lightPanel(image);
  const segments = [
    { source: 0, size: 1, target: 0, length: PICKUP_ATLAS_PADDING },
    { source: 0, size: PICKUP_ICON_SIZE, target: PICKUP_ATLAS_PADDING, length: PICKUP_ICON_SIZE },
    {
      source: PICKUP_ICON_SIZE - 1,
      size: 1,
      target: PICKUP_ATLAS_PADDING + PICKUP_ICON_SIZE,
      length: PICKUP_ATLAS_PADDING,
    },
  ];
  for (const x of segments) {
    for (const y of segments) {
      context.drawImage(
        icon,
        x.source,
        y.source,
        x.size,
        y.size,
        column * PICKUP_ATLAS_STRIDE + x.target,
        row * PICKUP_ATLAS_STRIDE + y.target,
        x.length,
        y.length,
      );
    }
  }
}
await writeFile(new URL(`../public/${PICKUP_ATLAS_PATH}`, import.meta.url), encodeWebp(canvas));
console.log(`public/${PICKUP_ATLAS_PATH}`);
