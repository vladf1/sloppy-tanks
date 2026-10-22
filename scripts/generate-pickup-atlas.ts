import { createCanvas, loadImage, type Canvas, type Image } from "@napi-rs/canvas";
import { execFileSync } from "node:child_process";
import { writeFile } from "node:fs/promises";
import {
  Random,
  type PickupKind,
  PICKUP_ATLAS_PATH,
  PICKUP_ATLAS_PADDING,
  PICKUP_ATLAS_SIZE,
  PICKUP_ATLAS_STRIDE,
  PICKUP_ATLAS_TILES,
  PICKUP_BRASS_CELL,
  PICKUP_ICON_SIZE,
  PICKUP_MARKINGS,
  PICKUP_PAINTS,
  PICKUP_PALLET_CELL,
  PICKUP_SURFACES,
  PICKUP_SWATCH_CELL,
} from "./asset-data";
import { fbm, lattice, smooth } from "./texture-noise";

// Paint the pickups as military supply. Each kind's tile has three bands, top to
// bottom (`TILE_ROWS`, the regions `pickup_visuals.rs` maps): the side strip, the
// surface its body shows (weathered pine, olive drab, black fibre, rolled armour
// steel) with a band of the kind's paint near the top and grime where it meets the
// pallet; a strip of the kind's plain paint for tubes, caps and edges; and the lid,
// one big block of the kind's paint with a modest emblem stencilled off to one side
// and a two-line lot marking beside it, so the colour carries from the gameplay
// camera and the markings read as stores, not icons. The paint is worn enamel,
// never toy-bright:
// mottled, chipped back to the surface under it along the edges and where it is
// knocked, scratched, the grain of a wooden lid reading through, dust drifted over
// it and grime gathered at its rim. The stencil is sprayed through a cut card in
// stencil cream on dark paint and black on light paint: its bridges leave gaps
// across the strokes, overspray mists and speckles round them, and it has chipped
// with the paint. The repair chest's stencil is a spanner (no protected emblem).
// The fourth column holds the pallet's pine planks and swatches of gunmetal,
// rubber, webbing and bright steel; below the block, cartridge brass. Edge pixels
// are extruded into every gutter so filtering and mipmaps never sample a neighbour.
// Encoded lossy: the surfaces are noise, which lossless packs badly.

type Rgb = readonly [number, number, number];
type Context = ReturnType<Canvas["getContext"]>;
type Surface = (typeof PICKUP_SURFACES)[PickupKind];

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
/** Rows of a tile (the engine's `pickup_visuals.rs` regions): the side strip, the
 * plain paint strip and the lid; the band's rows in the side strip; the rows at the
 * strip's foot grimed where a side meets the pallet; the stencil's centre and
 * largest extent on the lid. */
const TILE_ROWS = { side: [0, 64], paint: [64, 96], lid: [96, 256] } as const;
const BAND = [9, 27] as const;
const FOOT = 10;
const SYMBOL_CENTER = [84, 174] as const;
const SYMBOL_MAX = [96, 88] as const;
/** The lot marking: its two lines' left edge, baselines and letter height. */
const MARKING_LEFT = 146;
const MARKING_LINES = [166, 200] as const;
const MARKING_SIZE = 25;
/** Board height of a pine surface (pixels): two boards to a side strip, five to a
 * lid. */
const BOARD = 32;
const CORNER = 24;
/** Paint lighter than this (relative luminance) takes a black stencil. */
const LIGHT_PAINT = 0.47;
/** How the lid's paint has worn: how much has chipped away across it and along its
 * edges, how far in from an edge the wear reaches, and how strongly the surface's
 * relief reads through. */
const PAINT_CHIPS = 0.045;
const EDGE_CHIPS = 0.22;
const EDGE_WEAR = 20;
const PAINT_RELIEF = 0.55;
/** How far the stencil's strokes are spread, so thin lines carry from above. */
const STROKE_SPREAD = 4;
/** The stencil card's bridges: where they cross the symbol (share of its width)
 * and how wide a gap they leave. */
const BRIDGES = [0.36, 0.68] as const;
const BRIDGE_WIDTH = 3.5;
/** Overspray round the stencil: the mist's blur, its strongest opacity and the
 * chance per misted pixel (scaled by the mist) of a speck of paint. */
const OVERSPRAY_BLUR = 6;
const OVERSPRAY_MIST = 0.22;
const OVERSPRAY_SPECKS = 0.22;
/** How much of the stencil's paint has chipped away. */
const STENCIL_CHIPS = 0.05;
/** How deep into a face its paint chips. */
const CHIP_DEPTH = 12;
const PINE: Rgb = [164, 124, 80];
const PALLET_PINE: Rgb = [176, 134, 88];
const OLIVE: Rgb = [100, 108, 66];
const CAN_BLACK: Rgb = [50, 54, 46];
const FIBRE: Rgb = [52, 44, 36];
const ARMOUR: Rgb = [116, 120, 122];
const STEEL: Rgb = [70, 74, 78];
/** Stencil cream, as stores are marked, and black for the light tan case. */
const STENCIL_CREAM: Rgb = [222, 212, 178];
const STENCIL_BLACK: Rgb = [34, 36, 32];
/** Dust settled on the supply and the sheet a drop is laid on, grime, and the
 * fresh wood of a snapped board. */
const DUST: Rgb = [148, 132, 106];
const GRIME: Rgb = [62, 56, 44];
/** Rust weeping from seams and chips. */
const RUST: Rgb = [112, 66, 38];
const ENCODE_QUALITY = "88";

const hex = (color: number): Rgb => [(color >> 16) & 255, (color >> 8) & 255, color & 255];
const css = ([r, g, b]: Rgb, alpha = 1) =>
  `rgba(${Math.round(r)}, ${Math.round(g)}, ${Math.round(b)}, ${alpha})`;
const scale = ([r, g, b]: Rgb, k: number): Rgb => [r * k, g * k, b * k];
const blend = (a: Rgb, b: Rgb, t: number): Rgb => [
  a[0] + (b[0] - a[0]) * t,
  a[1] + (b[1] - a[1]) * t,
  a[2] + (b[2] - a[2]) * t,
];
const edgeDistance = (x: number, y: number) => Math.min(x, y, SIZE - x, SIZE - y);

/** Weathered planks running along x: `count` boards with dark seams, grain, the
 * odd knot and nail heads near both ends, greyed and grimy toward the edges. */
function planks(seed: number, base: Rgb, count: number) {
  const grain = lattice(seed);
  const tone = lattice(seed + 1);
  const random = new Random(seed);
  const shades = Array.from({ length: count }, () => random.range(0.84, 1.06));
  const knots = Array.from({ length: 3 }, () => [random.range(40, 216), random.range(0, SIZE)]);
  const height = SIZE / count;
  return (x: number, y: number): Rgb => {
    const board = Math.min(count - 1, Math.floor(y / height));
    const within = y - board * height;
    const streak = fbm(grain, x * 0.012 + board * 7.1, y * 0.32, 0, 4);
    const blotch = fbm(tone, x * 0.03, y * 0.03, 0, 3);
    let k = shades[board] * (0.8 + 0.4 * streak) * (0.92 + 0.16 * blotch);
    for (const [kx, ky] of knots) {
      const d = Math.hypot((x - kx) * 0.6, y - ky);
      if (d < 9) k *= 0.62 + 0.38 * smooth(3, 9, d);
    }
    // The seam between boards and the bevel catching light on the next one.
    if (within < 2.2) k *= 0.4;
    else if (within < 4) k *= 1.1;
    else if (within > height - 2) k *= 0.74;
    for (const nail of [16, SIZE - 16]) {
      if (Math.hypot(x - nail, within - height / 2) < 2.6) k = 0.42;
    }
    // Sun-greyed and grimy where hands and the ground touch the edges.
    const edge = 1 - smooth(0, EDGE_WEAR, edgeDistance(x, y));
    return blend(scale(base, k), scale([128, 120, 106], k), 0.35 * edge);
  };
}

/** Paint over steel: mottled, scuffed, and chipped back to dark steel only along
 * the edges, where it is handled, strapped and dragged. */
function paint(seed: number, base: Rgb, mottling = 0.18) {
  const mottle = lattice(seed);
  const chips = lattice(seed + 7);
  const dust = lattice(seed + 13);
  return (x: number, y: number): Rgb => {
    const k = 1 - mottling / 2 + mottling * fbm(mottle, x * 0.035, y * 0.035, 0, 4);
    const near = edgeDistance(x, y);
    const edge = 1 - smooth(2, EDGE_WEAR, near);
    const rim = 1 - smooth(1, CHIP_DEPTH, near);
    const chip = fbm(chips, x * 0.21, y * 0.21, 0, 3);
    // Chips: dark steel inside, a bright worn rim of paint round each one; along the
    // edges, with the odd scuff on the face.
    if (chip > 0.77 - 0.26 * rim) return scale(STEEL, 0.75 + 0.3 * k);
    if (chip > 0.73 - 0.26 * rim) return scale(base, 1.32 * k);
    const grime = edge * 0.38 + 0.1 * fbm(dust, x * 0.02, y * 0.02, 0, 3);
    return blend(scale(base, k), [78, 72, 58], grime);
  };
}

/** Paint pressed into horizontal ribs down the side strip, below the band: a lit
 * upper lip and a shadowed lower one every `pitch` pixels. */
function ribbed(seed: number, base: Rgb) {
  const surface = paint(seed, base);
  const pitch = 13;
  const [from, to] = [BAND[1] + 5, TILE_ROWS.side[1] - 4];
  return (x: number, y: number): Rgb => {
    const color = surface(x, y);
    if (y < from || y > to) return color;
    const at = (y - from) % pitch;
    if (at < 2) return scale(color, 1.25);
    if (at < 4) return scale(color, 0.68);
    return color;
  };
}

/** Black fibre shell canister: a spiral wrap of tarred paper, matte and dusty. */
function fibre(seed: number) {
  const wrap = lattice(seed);
  const dust = lattice(seed + 3);
  return (x: number, y: number): Rgb => {
    const seam = (x + y * 0.35) % 42 < 2.5 ? 0.62 : 1;
    const k = (0.85 + 0.3 * fbm(wrap, x * 0.05, y * 0.012, 0, 3)) * seam;
    const scuff = fbm(dust, x * 0.03, y * 0.03, 0, 3) * 0.25;
    return blend(scale(FIBRE, k), [120, 112, 98], scuff);
  };
}

/** Rolled armour steel: mill scale mottling, a few rust blooms and bright grinder
 * marks along the edges. */
function armour(seed: number) {
  const scaleNoise = lattice(seed);
  const rust = lattice(seed + 5);
  const marks = lattice(seed + 9);
  return (x: number, y: number): Rgb => {
    const k = 0.8 + 0.35 * fbm(scaleNoise, x * 0.03, y * 0.05, 0, 4);
    let color = scale(ARMOUR, k);
    const bloom = fbm(rust, x * 0.025, y * 0.025, 0, 4);
    if (bloom > 0.62) color = blend(color, [120, 72, 44], smooth(0.62, 0.74, bloom) * 0.7);
    const edge = 1 - smooth(1, 12, edgeDistance(x, y));
    if (edge > 0 && fbm(marks, x * 0.2, y * 0.02, 0, 2) > 0.5) {
      color = blend(color, [176, 178, 176], edge * 0.7);
    }
    return color;
  };
}

function surfaceOf(surface: Surface, seed: number, tint: Rgb): (x: number, y: number) => Rgb {
  switch (surface) {
    case "planks":
      return planks(seed, PINE, SIZE / BOARD);
    case "olive":
      return paint(seed, OLIVE);
    case "can":
      return ribbed(seed, CAN_BLACK);
    case "fibre":
      return fibre(seed);
    case "steel":
      return armour(seed);
    case "jerry":
      return paint(seed, OLIVE);
    case "painted":
      return ribbed(seed, scale(tint, 0.86));
  }
}

const luminance = ([r, g, b]: Rgb) => (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;

/** The stencil's paint: black on light paint, cream on dark. */
function stencilPaint(kind: PickupKind): Rgb {
  return luminance(hex(PICKUP_PAINTS[kind])) > LIGHT_PAINT ? STENCIL_BLACK : STENCIL_CREAM;
}

/** Distance (pixels) from `x, y` to the nearest edge of the tile's band `rows`. */
const rowsEdge = (x: number, y: number, [top, bottom]: readonly [number, number]) =>
  Math.min(x, SIZE - x, y - top, bottom - y);

/** Worn enamel in `color` over the rows `rows` of `c`, on whatever surface is there:
 * mottled and faded unevenly, the surface's relief (a lid's boards and grain, a
 * jerry can's pressing) reading through, chipped back to the surface where it is
 * knocked and more along the edges, scratched, and grimed toward the rim. */
function paintLayer(
  c: Context,
  rows: readonly [number, number],
  color: Rgb,
  seed: number,
  wear = 1,
) {
  const image = c.getImageData(0, 0, SIZE, SIZE);
  const mottle = lattice(seed);
  const chips = lattice(seed + 3);
  const scratches = lattice(seed + 5);
  const grime = lattice(seed + 7);
  // The surface's mean value across the rows, so its relief is measured from it.
  let total = 0;
  let count = 0;
  for (let y = rows[0]; y < rows[1]; y += 4) {
    for (let x = 0; x < SIZE; x += 4) {
      const i = (y * SIZE + x) * 4;
      total += luminance([image.data[i], image.data[i + 1], image.data[i + 2]]);
      count++;
    }
  }
  const level = Math.max(0.04, total / count);
  const grey = luminance(color) * 255;
  const enamel = blend(color, [grey, grey, grey], 0.24);
  for (let y = rows[0]; y < rows[1]; y++) {
    for (let x = 0; x < SIZE; x++) {
      const i = (y * SIZE + x) * 4;
      const under: Rgb = [image.data[i], image.data[i + 1], image.data[i + 2]];
      const near = rowsEdge(x + 0.5, y + 0.5, rows);
      const edge = 1 - smooth(1, EDGE_WEAR, near);
      // Chips: the surface showing through, ringed by a lighter worn lip, many
      // along the edges and the odd knock across the face.
      const chip = fbm(chips, x * 0.14, y * 0.14, 0, 3) + EDGE_CHIPS * edge * edge;
      const bare = smooth(0.78 - PAINT_CHIPS * wear, 0.8 - PAINT_CHIPS * wear, chip);
      const lip = smooth(0.74 - PAINT_CHIPS * wear, 0.77 - PAINT_CHIPS * wear, chip) - bare;
      const relief = Math.min(1.15, Math.max(0.5, luminance(under) / level)) ** PAINT_RELIEF;
      const fade = 0.86 + 0.2 * fbm(mottle, x * 0.025, y * 0.025, 0, 4);
      let painted = scale(enamel, relief * fade * (1 + 0.18 * lip));
      // Fine scratches along the lid, catching light.
      if (fbm(scratches, x * 0.02, y * 0.6, 0, 2) > 0.74) painted = scale(painted, 1.12);
      // Grime toward the rim and in drifts, dark and warm.
      const dirt = 0.42 * edge + 0.22 * smooth(0.5, 0.85, fbm(grime, x * 0.03, y * 0.03, 0, 3));
      painted = blend(painted, GRIME, dirt);
      image.data.set(
        blend(painted, scale(under, 0.85), bare).map((v) => Math.round(v)),
        i,
      );
    }
  }
  c.putImageData(image, 0, 0);
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

/** A stencil's shape on transparency in its ink, and the bounds of its strokes. */
interface Stencil {
  stencil: Canvas;
  bounds: { x: number; y: number; width: number; height: number };
}

/** The bounds of the opaque strokes on `canvas`. */
function strokeBounds(canvas: Canvas) {
  const pixels = canvas.getContext("2d").getImageData(0, 0, SIZE, SIZE).data;
  let [minX, minY, maxX, maxY] = [SIZE, SIZE, 0, 0];
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      if (pixels[(y * SIZE + x) * 4 + 3] > 128) {
        minX = Math.min(minX, x);
        minY = Math.min(minY, y);
        maxX = Math.max(maxX, x);
        maxY = Math.max(maxY, y);
      }
    }
  }
  return { x: minX, y: minY, width: maxX - minX + 1, height: maxY - minY + 1 };
}

/** The repair chest's stencil: a double open-ended spanner lying across the lid, a
 * plain tool mark in place of a protected emblem. */
function spannerStencil(ink: Rgb): Stencil {
  const stencil = createCanvas(SIZE, SIZE);
  const c = stencil.getContext("2d");
  c.translate(SIZE / 2, SIZE / 2);
  c.rotate(-Math.PI / 5);
  c.fillStyle = css(ink);
  const length = 150;
  const shank = 22;
  c.fillRect(-length / 2, -shank / 2, length, shank);
  for (const end of [-1, 1]) {
    c.beginPath();
    c.arc((end * length) / 2, 0, 34, 0, Math.PI * 2);
    c.fill();
  }
  // The jaws: a slot cut into each head, opening outward and a little askew.
  c.globalCompositeOperation = "destination-out";
  for (const end of [-1, 1]) {
    c.save();
    c.translate((end * length) / 2, 0);
    c.rotate(end * 0.35);
    c.fillRect(end > 0 ? 6 : -60, -12, 54, 24);
    c.beginPath();
    c.arc(end * 6, 0, 12, 0, Math.PI * 2);
    c.fill();
    c.restore();
  }
  return { stencil, bounds: strokeBounds(stencil) };
}

/** The source pictogram as a stencil on transparency: one colour, its accents cut
 * out as the stencil's bridges, cropped to its bounds. */
function extractStencil(image: Image, tint: Rgb): Stencil {
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

/** Wear a painted layer: knock it back where the noise runs high, more toward the
 * layer's `edge` falloff (a function of the pixel giving 0..1). A sprayed stencil
 * keeps crisp, solid strokes; only its fringe frays. */
function weather(
  canvas: Canvas,
  seed: number,
  amount: number,
  edge: (x: number, y: number) => number,
  size = SIZE,
) {
  const c = canvas.getContext("2d");
  const image = c.getImageData(0, 0, size, size);
  const noise = lattice(seed);
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const wear = fbm(noise, x * 0.09, y * 0.09, 0, 3) + 0.3 * edge(x, y);
      const i = (y * size + x) * 4 + 3;
      image.data[i] = Math.round(image.data[i] * (1 - smooth(0.78 - amount, 0.82 - amount, wear)));
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

/** Where and how large a stencil is sprayed: its centre, its largest extent, how
 * far its strokes are spread so thin lines carry from above, the overspray's blur
 * and the width of the card's bridges, all in pixels. */
interface Spray {
  center: readonly [number, number];
  max: readonly [number, number];
  spread: number;
  blur: number;
  bridge: number;
}

/** The source pictogram sprayed in `ink` through a cut card onto a transparent
 * canvas `size` wide: overspray misting and speckling round it, then solid paint
 * with the card's bridges left bare across the strokes, its fringe frayed and the
 * odd chip knocked out. */
function sprayStencil(shape: Stencil, at: Spray, size: number, seed: number) {
  const { stencil, bounds } = shape;
  const fit = Math.min(at.max[0] / bounds.width, at.max[1] / bounds.height);
  const [width, height] = [bounds.width * fit, bounds.height * fit];
  const [left, top] = [at.center[0] - width / 2, at.center[1] - height / 2];
  const layer = (blur: number, spread: number) => {
    const canvas = createCanvas(size, size);
    const context = canvas.getContext("2d");
    context.filter = blur ? `blur(${blur}px)` : "none";
    for (let step = 0; step < 12; step++) {
      const angle = (step / 12) * Math.PI * 2;
      context.drawImage(
        stencil,
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
        left + Math.cos(angle) * spread,
        top + Math.sin(angle) * spread,
        width,
        height,
      );
    }
    return canvas;
  };
  const solid = layer(0, at.spread);
  const mist = layer(at.blur, at.spread + at.blur / 3);
  const cut = solid.getContext("2d");
  cut.globalCompositeOperation = "destination-out";
  for (const along of BRIDGES) {
    cut.fillRect(left + width * along - at.bridge / 2, 0, at.bridge, size);
  }
  weather(solid, seed + 23, STENCIL_CHIPS, () => 0, size);
  // The mist and its speckles, under the paint.
  const mistPixels = mist.getContext("2d").getImageData(0, 0, size, size);
  const specks = new Random(seed + 29);
  for (let i = 3; i < mistPixels.data.length; i += 4) {
    const haze = mistPixels.data[i] / 255;
    const speck = haze > 0.04 && haze < 0.7 && specks.next() < OVERSPRAY_SPECKS * haze;
    mistPixels.data[i] = Math.round(255 * Math.max(OVERSPRAY_MIST * haze, speck ? 0.75 : 0));
  }
  const sprayed = createCanvas(size, size);
  const s = sprayed.getContext("2d");
  s.putImageData(mistPixels, 0, 0);
  s.drawImage(solid, 0, 0);
  return sprayed;
}

const TILE_SPRAY: Spray = {
  center: SYMBOL_CENTER,
  max: SYMBOL_MAX,
  spread: STROKE_SPREAD,
  blur: OVERSPRAY_BLUR,
  bridge: BRIDGE_WIDTH,
};

/** The jerry can's pressed X and border on its face (the lid band), lit from the
 * top left, on `surface`. */
function pressed(surface: (x: number, y: number) => Rgb) {
  const [top, bottom] = TILE_ROWS.lid;
  const inset = 14;
  return (x: number, y: number): Rgb => {
    const color = surface(x, y);
    if (y < top) return color;
    const u = (x - inset) / (SIZE - 2 * inset);
    const v = (y - top - inset) / (bottom - top - 2 * inset);
    if (u < 0 || v < 0 || u > 1 || v > 1) return color;
    const [w, h] = [SIZE - 2 * inset, bottom - top - 2 * inset];
    const d1 = (u - v) * Math.min(w, h);
    const d2 = (u + v - 1) * Math.min(w, h);
    const border = Math.min(u * w, v * h, (1 - u) * w, (1 - v) * h);
    for (const d of [d1, d2]) {
      if (Math.abs(d) < 8) return scale(color, d < 0 ? 1.3 : 0.66);
    }
    if (border < 5) return scale(color, border < 2.5 ? 1.25 : 0.7);
    return color;
  };
}

/** The kind's lot marking as a stencil: two short lines of block capitals. */
function markingStencil(kind: PickupKind, ink: Rgb): Stencil {
  const stencil = createCanvas(SIZE, SIZE);
  const c = stencil.getContext("2d");
  c.fillStyle = css(ink);
  c.font = `bold ${MARKING_SIZE}px sans-serif`;
  PICKUP_MARKINGS[kind].forEach((line, index) => {
    c.fillText(line, MARKING_LEFT, MARKING_LINES[index]);
  });
  return { stencil, bounds: strokeBounds(stencil) };
}

/** The marking is sprayed where it was cut, no wider than `MARKING_MAX`, its
 * strokes barely spread and its overspray finer than the emblem's. */
const MARKING_MAX = [100, 64] as const;
function markingSpray({ x, y, width, height }: Stencil["bounds"]): Spray {
  return {
    center: [x + Math.min(width, MARKING_MAX[0]) / 2, y + height / 2],
    max: [Math.min(width, MARKING_MAX[0]), Math.min(height, MARKING_MAX[1])],
    spread: 0.6,
    blur: 3,
    bridge: 2,
  };
}

/** Rust weeping down from the side strip's band and from chips: thin streaks
 * where the noise runs high, darkest at their head. */
function rustStreaks(c: Context, seed: number) {
  const image = c.getImageData(0, 0, SIZE, SIZE);
  const streaks = lattice(seed);
  const [, sideBottom] = TILE_ROWS.side;
  for (let y = BAND[1]; y < sideBottom; y++) {
    const fall = 1 - (y - BAND[1]) / (sideBottom - BAND[1]);
    for (let x = 0; x < SIZE; x++) {
      const streak = smooth(0.62, 0.8, fbm(streaks, x * 0.18, y * 0.012, 0, 3)) * fall;
      if (streak <= 0) continue;
      const i = (y * SIZE + x) * 4;
      const under: Rgb = [image.data[i], image.data[i + 1], image.data[i + 2]];
      image.data.set(
        blend(under, RUST, 0.45 * streak).map((v) => Math.round(v)),
        i,
      );
    }
  }
  c.putImageData(image, 0, 0);
}

function craftTile(kind: PickupKind, image: Image) {
  const surface: Surface = PICKUP_SURFACES[kind];
  const tint = hex(PICKUP_PAINTS[kind]);
  const seed = 0x5a7e + PICKUP_PAINTS[kind];
  const canvas = createCanvas(SIZE, SIZE);
  const c = canvas.getContext("2d");
  const body = surfaceOf(surface, seed, tint);
  fill(c, surface === "jerry" ? pressed(body) : body);
  // The band in the kind's paint across the side strip, the strip of plain paint
  // and the painted lid.
  paintLayer(c, BAND, tint, seed + 11, 1.4);
  paintLayer(c, TILE_ROWS.paint, tint, seed + 13);
  paintLayer(c, TILE_ROWS.lid, tint, seed + 17);
  if (surface === "planks") cornerPlates(c);
  const ink = stencilPaint(kind);
  const shape = kind === "repair" ? spannerStencil(ink) : extractStencil(image, ink);
  c.drawImage(sprayStencil(shape, TILE_SPRAY, SIZE, seed), 0, 0);
  const marking = markingStencil(kind, ink);
  c.drawImage(sprayStencil(marking, markingSpray(marking.bounds), SIZE, seed + 41), 0, 0);
  rustStreaks(c, seed + 37);
  settleDust(c, seed + 31);
  return canvas;
}

/** Dust and grime over a whole tile: drifts where the noise runs high and dust
 * blown over everything, rain streaks down the side strip, and grime banked up at
 * the strip's foot, where a side meets the pallet. */
function settleDust(c: Context, seed: number) {
  const image = c.getImageData(0, 0, SIZE, SIZE);
  const drift = lattice(seed);
  const rain = lattice(seed + 1);
  const [sideTop, sideBottom] = TILE_ROWS.side;
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      const side = y < sideBottom;
      const low = side ? smooth(sideTop + 20, sideBottom, y) : 0;
      const foot = side ? smooth(sideBottom - FOOT, sideBottom, y) : 0;
      // Dust lies thicker on the lid, which faces up.
      const lid = y >= TILE_ROWS.lid[0] ? 0.08 : 0;
      const amount =
        0.08 + lid + 0.28 * smooth(0.42, 0.85, fbm(drift, x * 0.025, y * 0.025, 0, 4)) + 0.2 * low;
      const streak = side
        ? smooth(0.55, 0.75, fbm(rain, x * 0.12, y * 0.02, 0, 3)) * 0.18 * (1 - low)
        : 0;
      const i = (y * SIZE + x) * 4;
      for (let k = 0; k < 3; k++) {
        const dusty = image.data[i + k] + (DUST[k] - image.data[i + k]) * amount;
        const grimy = dusty + (GRIME[k] - dusty) * 0.55 * foot;
        image.data[i + k] = grimy * (1 - streak);
      }
    }
  }
  c.putImageData(image, 0, 0);
}

/** The pallet's pine planks, one board to a row with its grain along the row:
 * weathered but still wood, a little greyed, with deep grain lines, end grain
 * darkening toward both ends, grime and the odd oil stain. */
function palletCell() {
  const canvas = createCanvas(SIZE, SIZE);
  const boards = planks(0x9a11e7, PALLET_PINE, 4);
  const silver = lattice(0x9a11f3);
  const lines = lattice(0x9a11f5);
  const stains = lattice(0x9a11f7);
  fill(canvas.getContext("2d"), (x, y) => {
    const wood = boards(x, y);
    const grey = 0.12 + 0.22 * fbm(silver, x * 0.02, y * 0.08, 0, 3);
    const value = (wood[0] + wood[1] + wood[2]) / 3;
    let color = blend(wood, [value * 1.02, value, value * 0.96], grey);
    // Fine grain lines running down each board, and its ends darkened by handling.
    const line = fbm(lines, x * 0.006, y * 0.9, 0, 3);
    color = scale(color, 0.82 + 0.3 * smooth(0.3, 0.7, line));
    color = scale(color, 0.78 + 0.22 * smooth(0, 40, Math.min(x, SIZE - x)));
    const stain = smooth(0.66, 0.76, fbm(stains, x * 0.02, y * 0.03, 0, 3));
    return blend(color, [52, 44, 36], stain * 0.55);
  });
  return canvas;
}

/** Four swatches: gunmetal (top left), black rubber (top right), olive webbing
 * (bottom left) and bright worn steel (bottom right). */
function swatchCell() {
  const canvas = createCanvas(SIZE, SIZE);
  const half = SIZE / 2;
  const scratches = lattice(0x7e11);
  const grain = lattice(0x7e13);
  fill(canvas.getContext("2d"), (x, y) => {
    const left = x < half;
    const top = y < half;
    const n = fbm(grain, x * 0.06, y * 0.06, 0, 3);
    if (top && left) {
      const scratch = fbm(scratches, x * 0.4, y * 0.03, 0, 2) > 0.66 ? 1.35 : 1;
      return scale(STEEL, (0.85 + 0.3 * n) * scratch);
    }
    if (top) return scale([34, 34, 36], 0.85 + 0.3 * n);
    if (left) {
      const weave = (Math.floor(x / 3) + Math.floor(y / 3)) % 2 ? 0.88 : 1.06;
      return scale([90, 94, 62], weave * (0.9 + 0.2 * n));
    }
    return scale([156, 158, 154], 0.82 + 0.3 * fbm(grain, x * 0.5, y * 0.02, 0, 2));
  });
  return canvas;
}

/** A cartridge from base to tip, the same all round: a dark rim, a tarnished
 * brass case darkening toward its shoulder, the copper driving band and the olive
 * drab projectile with its faded yellow filling band and a dark steel nose. */
function brassCell() {
  const canvas = createCanvas(SIZE, SIZE);
  const streak = lattice(0xb4a55);
  const tarnish = lattice(0xb4a57);
  fill(canvas.getContext("2d"), (x, y) => {
    const v = 1 - y / SIZE;
    const n = fbm(streak, x * 0.08, y * 0.01, 0, 3);
    const dull = smooth(0.4, 0.8, fbm(tarnish, x * 0.05, y * 0.05, 0, 3)) * 0.45;
    if (v < 0.05) return scale([104, 80, 44], 0.9 + 0.2 * n);
    if (v < 0.6) {
      const shoulder = smooth(0.45, 0.6, v);
      const brass = scale(blend([192, 150, 78], [150, 110, 54], shoulder), 0.86 + 0.24 * n);
      return blend(brass, [96, 84, 56], dull);
    }
    if (v < 0.66) return scale([140, 84, 52], 0.9 + 0.2 * n);
    if (v > 0.93) return scale([62, 64, 60], 0.9 + 0.2 * n);
    if (v > 0.78 && v < 0.83) return scale([176, 156, 70], 0.86 + 0.2 * n);
    return blend(scale([92, 98, 70], 0.86 + 0.24 * n), DUST, 0.12 * n);
  });
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
place(swatchCell(), ...PICKUP_SWATCH_CELL);
place(brassCell(), ...PICKUP_BRASS_CELL);
const webp = execFileSync(
  "cwebp",
  ["-quiet", "-q", ENCODE_QUALITY, "-alpha_q", "100", "-m", "6", "-o", "-", "--", "-"],
  { input: atlas.toBuffer("image/png"), maxBuffer: 32 * 1024 * 1024 },
);
await writeFile(new URL(`../public/${PICKUP_ATLAS_PATH}`, import.meta.url), webp);
console.log(`public/${PICKUP_ATLAS_PATH}`);
