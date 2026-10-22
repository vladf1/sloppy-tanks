import { createCanvas, loadImage, type Canvas, type Image } from "@napi-rs/canvas";
import { execFileSync } from "node:child_process";
import { writeFile } from "node:fs/promises";
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
  PICKUP_PAINT_CELL,
  PICKUP_PALLET_CELL,
  PICKUP_SHAPES,
} from "./asset-data";
import { fbm, lattice, smooth } from "./texture-noise";

// Paint the pickups as military supply: each kind's tile is the surface of what it
// ships in (weathered pine planks for ammunition crates, olive drab steel with
// pressed ribs for cases, an embossed X for the jerry can), with steel corner
// plates, a band in the kind's colour across the top and the source pictogram
// sprayed on as a stencil in that colour. The fourth column holds the pallet's bare
// planks and the spray paint marking the ground (white, its ragged edges in alpha).
// Edge pixels are extruded into every gutter so filtering and mipmaps never sample
// a neighbour. Encoded lossy: the surfaces are noise, which lossless packs badly.

type Rgb = readonly [number, number, number];
type Context = ReturnType<Canvas["getContext"]>;
type Shape = (typeof PICKUP_SHAPES)[PickupKind];
const KIND_COLORS: Record<PickupKind, number> = {
  ...Object.fromEntries(SPECIAL_AMMO.map(({ weapon, color }) => [weapon, color])),
  ...PICKUP_COLORS,
} as Record<PickupKind, number>;

const SIZE = PICKUP_ICON_SIZE;
/** The source's symbol area: inside its frame, above its printed label, clear of
 * the corner studs. */
const SYMBOL_AREA = { left: 40, top: 42, right: 216, bottom: 190 } as const;
/** A pixel of the navy panel every source icon is drawn on. */
const PANEL_SEED = [128, 40] as const;
/** RGB distance from the panel at which a source pixel is fully symbol. */
const SYMBOL_DISTANCE = 140;
/** Saturation above which a symbol pixel is a coloured accent, cut out of the
 * stencil as one of its bridges. */
const ACCENT_SATURATION = 0.4;
/** Layout of a tile in pixels (the engine's `pickup_visuals.rs` maps the same
 * regions): the stencil's longest side and centre, the band's rows, and the steel
 * corner plates. */
const SYMBOL_SIZE = 146;
const SYMBOL_CENTER = [128, 152] as const;
/** How far the stencil's strokes are spread, so thin lines carry from above. */
const STROKE_SPREAD = 2.5;
const BAND = [26, 62] as const;
const CORNER = 26;
const PINE: Rgb = [168, 126, 80];
const PALLET_PINE: Rgb = [186, 150, 104];
const OLIVE: Rgb = [92, 100, 62];
const CASE_GREEN: Rgb = [84, 94, 72];
const STEEL: Rgb = [74, 78, 82];
const ENCODE_QUALITY = "88";

const hex = (color: number): Rgb => [(color >> 16) & 255, (color >> 8) & 255, color & 255];
const css = ([r, g, b]: Rgb, alpha = 1) =>
  `rgba(${Math.round(r)}, ${Math.round(g)}, ${Math.round(b)}, ${alpha})`;
const scale = ([r, g, b]: Rgb, k: number): Rgb => [r * k, g * k, b * k];

/** Weathered planks running along x: `count` boards with dark seams, grain, the
 * odd knot and nail heads near both ends. */
function planks(seed: number, base: Rgb, count: number) {
  const grain = lattice(seed);
  const tone = lattice(seed + 1);
  const random = new Random(seed);
  const shades = Array.from({ length: count }, () => random.range(0.82, 1.08));
  const knots = Array.from({ length: 3 }, () => [random.range(40, 216), random.range(0, SIZE)]);
  const height = SIZE / count;
  return (x: number, y: number): Rgb => {
    const board = Math.min(count - 1, Math.floor(y / height));
    const within = y - board * height;
    const streak = fbm(grain, x * 0.012 + board * 7.1, y * 0.32, 0, 4);
    const blotch = fbm(tone, x * 0.03, y * 0.03, 0, 3);
    let k = shades[board] * (0.78 + 0.42 * streak) * (0.9 + 0.2 * blotch);
    for (const [kx, ky] of knots) {
      const d = Math.hypot((x - kx) * 0.6, y - ky);
      if (d < 9) k *= 0.62 + 0.38 * smooth(3, 9, d);
    }
    // The seam between boards and the bevel catching light on the next one.
    if (within < 2.2) k *= 0.38;
    else if (within < 4) k *= 1.12;
    else if (within > height - 2) k *= 0.72;
    for (const nail of [16, SIZE - 16]) {
      if (Math.hypot(x - nail, within - height / 2) < 2.6) k = 0.42;
    }
    return scale(base, k);
  };
}

/** Olive drab paint over steel: mottled, scuffed, chipped back to dark steel near
 * the edges. */
function paint(seed: number, base: Rgb) {
  const mottle = lattice(seed);
  const chips = lattice(seed + 7);
  return (x: number, y: number): Rgb => {
    const k = 0.88 + 0.22 * fbm(mottle, x * 0.04, y * 0.04, 0, 4);
    const edge = Math.min(x, y, SIZE - x, SIZE - y);
    const chipped = fbm(chips, x * 0.09, y * 0.09, 0, 3) + (1 - smooth(4, 40, edge)) * 0.25;
    if (chipped > 0.74) return scale(STEEL, 0.75 + 0.3 * k);
    return scale(base, k);
  };
}

/** A steel case's paint pressed into horizontal ribs: a lit upper lip and a
 * shadowed lower one every `pitch` pixels below the band. */
function ribbed(seed: number) {
  const surface = paint(seed, CASE_GREEN);
  const pitch = 46;
  return (x: number, y: number): Rgb => {
    const color = surface(x, y);
    if (y < BAND[1] + 10 || y > SIZE - 10) return color;
    const at = (y - BAND[1] - 10) % pitch;
    if (at < 3) return scale(color, 1.3);
    if (at < 6) return scale(color, 0.62);
    return color;
  };
}

/** The jerry can's pressed X and border, lit from the top left. */
function embossed(seed: number) {
  const surface = paint(seed, OLIVE);
  const inset = 0.1 * SIZE;
  const span = SIZE - 2 * inset;
  return (x: number, y: number): Rgb => {
    const color = surface(x, y);
    const u = (x - inset) / span;
    const v = (y - inset) / span;
    if (u < 0 || v < 0 || u > 1 || v > 1) return color;
    // Signed distances to the two diagonals and to the border, in pixels.
    const d1 = ((u - v) * span) / Math.SQRT2;
    const d2 = ((u + v - 1) * span) / Math.SQRT2;
    const border = Math.min(u, v, 1 - u, 1 - v) * span;
    for (const d of [d1, d2]) {
      if (Math.abs(d) < 7) return scale(color, d < 0 ? 1.28 : 0.7);
    }
    if (border < 5) return scale(color, border < 2.5 ? 1.25 : 0.72);
    return color;
  };
}

function fill(c: Context, surface: (x: number, y: number) => Rgb) {
  const image = c.createImageData(SIZE, SIZE);
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      const [r, g, b] = surface(x + 0.5, y + 0.5);
      image.data.set([r, g, b, 255], (y * SIZE + x) * 4);
    }
  }
  c.putImageData(image, 0, 0);
}

/** The source pictogram as a stencil on transparency: one colour, its accents cut
 * out as the stencil's bridges, cropped to its bounds. */
function extractStencil(image: Image, tint: Rgb) {
  const source = createCanvas(SIZE, SIZE);
  const sourceContext = source.getContext("2d");
  sourceContext.drawImage(image, 0, 0);
  const pixels = sourceContext.getImageData(0, 0, SIZE, SIZE).data;
  const seed = (PANEL_SEED[1] * SIZE + PANEL_SEED[0]) * 4;
  const panel = [pixels[seed], pixels[seed + 1], pixels[seed + 2]];
  const stencil = createCanvas(SIZE, SIZE);
  const context = stencil.getContext("2d");
  const out = context.createImageData(SIZE, SIZE);
  let [minX, minY, maxX, maxY] = [SIZE, SIZE, 0, 0];
  for (let y = SYMBOL_AREA.top; y < SYMBOL_AREA.bottom; y++) {
    for (let x = SYMBOL_AREA.left; x < SYMBOL_AREA.right; x++) {
      const i = (y * SIZE + x) * 4;
      const rgb = [pixels[i], pixels[i + 1], pixels[i + 2]];
      const alpha = Math.min(1, Math.hypot(...rgb.map((c, k) => c - panel[k])) / SYMBOL_DISTANCE);
      if (alpha < 0.04) continue;
      const max = Math.max(...rgb);
      const saturation = max === 0 ? 0 : (max - Math.min(...rgb)) / max;
      if (saturation > ACCENT_SATURATION) continue;
      out.data.set([...tint, Math.round(alpha * 255)], i);
      if (alpha > 0.5) {
        minX = Math.min(minX, x);
        minY = Math.min(minY, y);
        maxX = Math.max(maxX, x);
        maxY = Math.max(maxY, y);
      }
    }
  }
  context.putImageData(out, 0, 0);
  return { stencil, bounds: { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 } };
}

/** Wear a painted layer: knock out small chips where the noise runs high. */
function weather(canvas: Canvas, seed: number, amount: number) {
  const c = canvas.getContext("2d");
  const image = c.getImageData(0, 0, SIZE, SIZE);
  const noise = lattice(seed);
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      const wear = fbm(noise, x * 0.11, y * 0.11, 0, 3);
      const i = (y * SIZE + x) * 4 + 3;
      image.data[i] = Math.round(image.data[i] * (1 - smooth(0.7 - amount, 0.75 - amount, wear)));
    }
  }
  c.putImageData(image, 0, 0);
}

function cornerPlates(c: Context) {
  for (const [x, y] of [
    [0, 0],
    [SIZE - CORNER, 0],
    [0, SIZE - CORNER],
    [SIZE - CORNER, SIZE - CORNER],
  ]) {
    c.fillStyle = css(STEEL);
    c.fillRect(x, y, CORNER, CORNER);
    c.fillStyle = css(scale(STEEL, 1.45), 0.9);
    c.fillRect(x, y, CORNER, 2);
    c.fillRect(x, y, 2, CORNER);
    c.fillStyle = css(scale(STEEL, 0.5), 0.9);
    c.fillRect(x, y + CORNER - 2, CORNER, 2);
    c.fillRect(x + CORNER - 2, y, 2, CORNER);
    const [cx, cy] = [x + CORNER / 2, y + CORNER / 2];
    c.fillStyle = css(scale(STEEL, 0.45));
    c.beginPath();
    c.arc(cx + 0.8, cy + 0.8, 3.6, 0, Math.PI * 2);
    c.fill();
    c.fillStyle = css(scale(STEEL, 1.6));
    c.beginPath();
    c.arc(cx - 0.4, cy - 0.4, 2.6, 0, Math.PI * 2);
    c.fill();
  }
}

function craftTile(kind: PickupKind, image: Image) {
  const shape: Shape = PICKUP_SHAPES[kind];
  const tint = hex(KIND_COLORS[kind]);
  const seed = 0x5a7e + KIND_COLORS[kind];
  const canvas = createCanvas(SIZE, SIZE);
  const c = canvas.getContext("2d");
  const surface =
    shape === "crate" ? planks(seed, PINE, 5) : shape === "case" ? ribbed(seed) : embossed(seed);
  fill(c, surface);
  // The band in the kind's colour across the top, worn back in places.
  const band = createCanvas(SIZE, SIZE);
  const b = band.getContext("2d");
  b.fillStyle = css(tint);
  b.fillRect(0, BAND[0], SIZE, BAND[1] - BAND[0]);
  b.fillStyle = css(scale(tint, 0.55));
  b.fillRect(0, BAND[0], SIZE, 2);
  b.fillRect(0, BAND[1] - 2, SIZE, 2);
  weather(band, seed + 11, 0.12);
  c.drawImage(band, 0, 0);
  // The stencil, sprayed: a faint overspray halo, then the paint, worn a little.
  const { stencil, bounds } = extractStencil(image, tint);
  const fit = SYMBOL_SIZE / Math.max(bounds.width, bounds.height);
  const [width, height] = [bounds.width * fit, bounds.height * fit];
  const sprayed = createCanvas(SIZE, SIZE);
  const s = sprayed.getContext("2d");
  const spray = (alpha: number, blur: number, spread: number) => {
    s.globalAlpha = alpha;
    s.shadowColor = blur ? css(tint, 0.9) : "transparent";
    s.shadowBlur = blur;
    for (let step = 0; step < 8; step++) {
      const angle = (step / 8) * Math.PI * 2;
      s.drawImage(
        stencil,
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        SYMBOL_CENTER[0] - width / 2 + Math.cos(angle) * spread,
        SYMBOL_CENTER[1] - height / 2 + Math.sin(angle) * spread,
        width,
        height,
      );
    }
  };
  spray(0.08, 6, STROKE_SPREAD);
  s.globalCompositeOperation = "source-over";
  spray(1, 0, STROKE_SPREAD);
  weather(sprayed, seed + 23, 0.08);
  c.drawImage(sprayed, 0, 0);
  if (shape !== "jerry") cornerPlates(c);
  return canvas;
}

function palletCell() {
  const canvas = createCanvas(SIZE, SIZE);
  fill(canvas.getContext("2d"), planks(0x9a11e7, PALLET_PINE, 4));
  return canvas;
}

/** White spray paint across the cell's middle, its edges ragged in alpha, with a
 * little overspray beyond them. */
function paintCell() {
  const canvas = createCanvas(SIZE, SIZE);
  const c = canvas.getContext("2d");
  const image = c.createImageData(SIZE, SIZE);
  const noise = lattice(0x5b1a7);
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      const across = Math.abs(y + 0.5 - SIZE / 2) / (SIZE / 2);
      const rough = fbm(noise, x * 0.05, y * 0.05, 0, 4);
      const coverage = 1 - smooth(0.62, 0.95, across + (rough - 0.5) * 0.5);
      const shade = 225 + 30 * rough;
      image.data.set([shade, shade, shade, Math.round(coverage * 255)], (y * SIZE + x) * 4);
    }
  }
  c.putImageData(image, 0, 0);
  return canvas;
}

const atlas = createCanvas(PICKUP_ATLAS_SIZE, PICKUP_ATLAS_SIZE);
const context = atlas.getContext("2d");
context.imageSmoothingEnabled = false;
context.fillStyle = css(scale(STEEL, 0.8));
context.fillRect(0, 0, PICKUP_ATLAS_SIZE, PICKUP_ATLAS_SIZE);
function place(cell: Canvas, column: number, row: number) {
  const segments = [
    { source: 0, size: 1, target: 0, length: PICKUP_ATLAS_PADDING },
    { source: 0, size: SIZE, target: PICKUP_ATLAS_PADDING, length: SIZE },
    {
      source: SIZE - 1,
      size: 1,
      target: PICKUP_ATLAS_PADDING + SIZE,
      length: PICKUP_ATLAS_PADDING,
    },
  ];
  // Clear first, so a cell with transparency keeps it in its gutter too.
  context.clearRect(
    column * PICKUP_ATLAS_STRIDE,
    row * PICKUP_ATLAS_STRIDE,
    PICKUP_ATLAS_STRIDE,
    PICKUP_ATLAS_STRIDE,
  );
  for (const x of segments) {
    for (const y of segments) {
      context.drawImage(
        cell,
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
for (const [kind, [column, row]] of Object.entries(PICKUP_ATLAS_TILES)) {
  const image = await loadImage(
    new URL(`../assets/texture-sources/pickups/${kind}.webp`, import.meta.url),
  );
  if (image.width !== SIZE || image.height !== SIZE) {
    throw new Error(`Unexpected pickup icon dimensions: ${kind}`);
  }
  place(craftTile(kind as PickupKind, image), column, row);
}
place(palletCell(), ...PICKUP_PALLET_CELL);
place(paintCell(), ...PICKUP_PAINT_CELL);
const webp = execFileSync(
  "cwebp",
  ["-quiet", "-q", ENCODE_QUALITY, "-alpha_q", "100", "-m", "6", "-o", "-", "--", "-"],
  { input: atlas.toBuffer("image/png"), maxBuffer: 32 * 1024 * 1024 },
);
await writeFile(new URL(`../public/${PICKUP_ATLAS_PATH}`, import.meta.url), webp);
console.log(`public/${PICKUP_ATLAS_PATH}`);
