import { createCanvas, loadImage, type Image } from "@napi-rs/canvas";
import { writeFile } from "node:fs/promises";
import { encodeWebp } from "./encode-webp";
import {
  PICKUP_COLORS,
  Random,
  SPECIAL_AMMO,
  type PickupKind,
  PICKUP_ATLAS_PATH,
  PICKUP_ATLAS_PADDING,
  PICKUP_ATLAS_SIZE,
  PICKUP_ATLAS_STRIDE,
  PICKUP_ATLAS_TILES,
  PICKUP_ICON_SIZE,
} from "./asset-data";

// Pack the lossless pictogram artwork offline into crafted tiles: a bevelled,
// worn gunmetal frame around a recessed panel lit from within in the kind's color,
// with the source pictogram (its printed label dropped) enlarged as a white symbol
// outlined in dark ink so it reads on any panel from the battle camera. Then
// extrude edge pixels into every gutter so filtering/mipmaps do not sample the
// neighboring pickup's icon.

type Rgb = readonly [number, number, number];
const KIND_COLORS: Record<PickupKind, number> = {
  ...Object.fromEntries(SPECIAL_AMMO.map(({ weapon, color }) => [weapon, color])),
  ...PICKUP_COLORS,
} as Record<PickupKind, number>;

/** The source's symbol area: inside its frame, above its printed label, clear of
 * the corner studs. */
const SYMBOL_AREA = { left: 40, top: 42, right: 216, bottom: 190 } as const;
/** A pixel of the navy panel every source icon is drawn on. */
const PANEL_SEED = [128, 40] as const;
/** RGB distance from the panel at which a source pixel is fully symbol. */
const SYMBOL_DISTANCE = 140;
/** Saturation above which a symbol pixel is a colored accent, drawn in ink. */
const ACCENT_SATURATION = 0.4;
/** The symbol's longest side in the tile, its centre, and its ink outline. */
const SYMBOL_SIZE = 168;
const SYMBOL_CENTER = [128, 126] as const;
const OUTLINE = 7;
/** Frame: outer bevel, flat band and the recess lip around the panel (pixels). */
const BEVEL = 7;
const PANEL_INSET = 22;
const FRAME_STEEL = "#4d5966";
const BEVEL_LIGHT = "#97a3b0";
const BEVEL_SIDE = "#7b8794";
const BEVEL_DARK = "#232b34";
const BEVEL_SHADE = "#2f3943";
const INK = [14, 22, 30] as const;

const hex = (color: number): Rgb => [(color >> 16) & 255, (color >> 8) & 255, color & 255];
const css = ([r, g, b]: Rgb, alpha = 1) =>
  `rgba(${Math.round(r)}, ${Math.round(g)}, ${Math.round(b)}, ${alpha})`;
const mix = (a: Rgb, b: Rgb, t: number): Rgb => [0, 1, 2].map((i) => a[i] + (b[i] - a[i]) * t) as unknown as Rgb;

/** The source pictogram as a white symbol with ink accents on transparency,
 * cropped to its bounds. */
function extractSymbol(image: Image) {
  const size = PICKUP_ICON_SIZE;
  const source = createCanvas(size, size);
  const sourceContext = source.getContext("2d");
  sourceContext.drawImage(image, 0, 0);
  const pixels = sourceContext.getImageData(0, 0, size, size).data;
  const seed = (PANEL_SEED[1] * size + PANEL_SEED[0]) * 4;
  const panel = [pixels[seed], pixels[seed + 1], pixels[seed + 2]];
  const symbol = createCanvas(size, size);
  const context = symbol.getContext("2d");
  const out = context.createImageData(size, size);
  let [minX, minY, maxX, maxY] = [size, size, 0, 0];
  for (let y = SYMBOL_AREA.top; y < SYMBOL_AREA.bottom; y++) {
    for (let x = SYMBOL_AREA.left; x < SYMBOL_AREA.right; x++) {
      const i = (y * size + x) * 4;
      const rgb = [pixels[i], pixels[i + 1], pixels[i + 2]];
      const alpha = Math.min(1, Math.hypot(...rgb.map((c, k) => c - panel[k])) / SYMBOL_DISTANCE);
      if (alpha < 0.04) continue;
      const max = Math.max(...rgb);
      const saturation = max === 0 ? 0 : (max - Math.min(...rgb)) / max;
      const color = saturation > ACCENT_SATURATION ? INK : [255, 255, 255];
      out.data.set([...color, Math.round(alpha * 255)], i);
      if (alpha > 0.5) {
        minX = Math.min(minX, x);
        minY = Math.min(minY, y);
        maxX = Math.max(maxX, x);
        maxY = Math.max(maxY, y);
      }
    }
  }
  context.putImageData(out, 0, 0);
  return { symbol, bounds: { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 } };
}

/** The symbol's silhouette in one flat color, for its outline and shadow. */
function silhouette(symbol: ReturnType<typeof createCanvas>, color: Rgb) {
  const canvas = createCanvas(symbol.width, symbol.height);
  const context = canvas.getContext("2d");
  context.drawImage(symbol, 0, 0);
  context.globalCompositeOperation = "source-in";
  context.fillStyle = css(color);
  context.fillRect(0, 0, canvas.width, canvas.height);
  return canvas;
}

function craftTile(kind: PickupKind, image: Image) {
  const size = PICKUP_ICON_SIZE;
  const tint = hex(KIND_COLORS[kind]);
  const canvas = createCanvas(size, size);
  const c = canvas.getContext("2d");
  // Gunmetal frame: lit from the top left, bevelled at the outer edge.
  c.fillStyle = FRAME_STEEL;
  c.fillRect(0, 0, size, size);
  const band = (color: string, points: number[][]) => {
    c.fillStyle = color;
    c.beginPath();
    c.moveTo(points[0][0], points[0][1]);
    for (const [x, y] of points.slice(1)) c.lineTo(x, y);
    c.closePath();
    c.fill();
  };
  const [a, b] = [BEVEL, size - BEVEL];
  band(BEVEL_LIGHT, [[0, 0], [size, 0], [b, a], [a, a]]);
  band(BEVEL_SIDE, [[0, 0], [a, a], [a, b], [0, size]]);
  band(BEVEL_DARK, [[0, size], [a, b], [b, b], [size, size]]);
  band(BEVEL_SHADE, [[size, 0], [size, size], [b, b], [b, a]]);
  // Worn edges: bright scuffs where the paint is chipped back to steel.
  const random = new Random(0x51c0de + KIND_COLORS[kind]);
  for (let i = 0; i < 70; i++) {
    const along = random.range(0, size);
    const depth = random.range(1, PANEL_INSET - 3);
    const side = Math.floor(random.next() * 4);
    const [x, y] = [
      [along, depth],
      [depth, along],
      [along, size - depth],
      [size - depth, along],
    ][side];
    c.fillStyle = random.next() < 0.6 ? "rgba(190, 200, 210, 0.35)" : "rgba(15, 20, 26, 0.35)";
    c.fillRect(x, y, random.range(1, side % 2 ? 2 : 6), random.range(1, side % 2 ? 6 : 2));
  }
  // Corner bolts.
  for (const [x, y] of [
    [14, 14],
    [size - 14, 14],
    [14, size - 14],
    [size - 14, size - 14],
  ]) {
    c.fillStyle = BEVEL_DARK;
    c.beginPath();
    c.arc(x + 0.8, y + 0.8, 4.6, 0, Math.PI * 2);
    c.fill();
    c.fillStyle = BEVEL_LIGHT;
    c.beginPath();
    c.arc(x - 0.4, y - 0.4, 3.6, 0, Math.PI * 2);
    c.fill();
  }
  // The recessed panel, glowing in the kind's color from its centre.
  const inner = size - PANEL_INSET * 2;
  const glow = c.createRadialGradient(128, 118, 8, 128, 128, inner * 0.78);
  glow.addColorStop(0, css(mix(tint, [255, 255, 255], 0.35)));
  glow.addColorStop(0.45, css(tint));
  glow.addColorStop(1, css(mix(tint, [0, 0, 0], 0.5)));
  c.fillStyle = glow;
  c.fillRect(PANEL_INSET, PANEL_INSET, inner, inner);
  // The lip's shadow falls into the recess from the top left; light catches the
  // bottom-right lip.
  const lip = 9;
  const shade = (x0: number, y0: number, x1: number, y1: number, alpha: number) => {
    const gradient = c.createLinearGradient(x0, y0, x1, y1);
    gradient.addColorStop(0, `rgba(0, 0, 0, ${alpha})`);
    gradient.addColorStop(1, "rgba(0, 0, 0, 0)");
    return gradient;
  };
  c.fillStyle = shade(0, PANEL_INSET, 0, PANEL_INSET + lip, 0.55);
  c.fillRect(PANEL_INSET, PANEL_INSET, inner, lip);
  c.fillStyle = shade(PANEL_INSET, 0, PANEL_INSET + lip, 0, 0.45);
  c.fillRect(PANEL_INSET, PANEL_INSET, lip, inner);
  c.fillStyle = "rgba(255, 255, 255, 0.28)";
  c.fillRect(PANEL_INSET, size - PANEL_INSET - 2, inner, 2);
  c.fillRect(size - PANEL_INSET - 2, PANEL_INSET, 2, inner);
  // The symbol, enlarged: a soft drop shadow, an ink outline, then white.
  const { symbol, bounds } = extractSymbol(image);
  const scale = SYMBOL_SIZE / Math.max(bounds.width, bounds.height);
  const width = bounds.width * scale;
  const height = bounds.height * scale;
  const place = (source: ReturnType<typeof createCanvas>, dx: number, dy: number) =>
    c.drawImage(
      source,
      bounds.x,
      bounds.y,
      bounds.width,
      bounds.height,
      SYMBOL_CENTER[0] - width / 2 + dx,
      SYMBOL_CENTER[1] - height / 2 + dy,
      width,
      height,
    );
  const ink = silhouette(symbol, INK);
  c.globalAlpha = 0.35;
  place(ink, 3, 9);
  c.globalAlpha = 1;
  for (let step = 0; step < 24; step++) {
    const angle = (step / 24) * Math.PI * 2;
    place(ink, Math.cos(angle) * OUTLINE, Math.sin(angle) * OUTLINE);
  }
  place(symbol, 0, 0);
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
  const icon = craftTile(kind as PickupKind, image);
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
