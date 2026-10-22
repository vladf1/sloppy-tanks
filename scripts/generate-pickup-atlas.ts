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
  PICKUP_BUNDLE_CELL,
  PICKUP_GLYPH_CELL,
  PICKUP_ICON_SIZE,
  PICKUP_PAINT_CELL,
  PICKUP_PAINTS,
  PICKUP_PALLET_CELL,
  PICKUP_SURFACES,
  PICKUP_SWATCH_CELL,
  PICKUP_TARP_CELL,
} from "./asset-data";
import { fbm, lattice, smooth } from "./texture-noise";

// Paint the pickups as military supply. Each kind's tile is the surface its supply
// shows (weathered pine, olive drab, black fibre, rolled armour steel, pressed jerry
// can steel, a ribbed olive chest, a tan transit case), with a narrow band of the
// kind's paint across the top and, below it, a broad stripe of the same paint brushed
// across the face, the way ammunition is colour coded. The source pictogram is
// sprayed over the stripe through a cut stencil, in stencil cream on dark paint and
// black on light paint: its bridges leave gaps across the strokes, overspray mists
// and speckles round them, and the paint has chipped. The medical chest carries its
// red cross on a worn white stripe instead. Wear gathers where hands, straps and the
// ground touch: chipped paint and grime along the edges, rain streaks down the
// sides, worn stripe ends, dust settled low on every face. The fourth column holds
// the pallet's pine planks, the spray paint marking the ground (white over a dark
// primer edge, its broken strokes and overspray in alpha, seamless across the cell
// so it bends once round a drop) and swatches of gunmetal, rubber, webbing and
// bright steel; below the block, cartridge brass, the canvas sheet a drop is laid on
// (shaded where the pallet stands on it), the nine stencils a restocking drop shows
// (a white stripe for the marking's tint with the pictogram sprayed dark across it)
// and the canvas its empty pallet is bundled under. Edge pixels are extruded into
// every gutter so filtering and mipmaps never sample a neighbour. Encoded lossy: the
// surfaces are noise, which lossless packs badly.

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
/** Layout of a tile in pixels (the engine's `pickup_visuals.rs` maps the same
 * regions): the plain strip and corner plates along the top, the band's rows, the
 * stripe's rows and the stencil's centre and largest extent below them. */
const CORNER = 26;
const BAND = [26, 62] as const;
const STRIPE = [100, 236] as const;
const SYMBOL_CENTER = [128, 168] as const;
const SYMBOL_MAX = [148, 112] as const;
/** How far a stripe's brushed edges wander (pixels) and how much of its paint has
 * worn through to the surface. */
const STRIPE_RAGGED = 5;
const STRIPE_WEAR = 0.07;
/** Paint lighter than this (relative luminance) takes a black stencil. */
const LIGHT_PAINT = 0.47;
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
/** How far in from a face's edge its paint chips and grime gathers. */
const EDGE_WEAR = 22;
/** How deep into a face its paint chips. */
const CHIP_DEPTH = 12;
/** Rows of a tile a side face shows above its foot (the engine's `SIDE` region):
 * dust banks up toward the last of them. */
const SIDE_FOOT = 96;
const PINE: Rgb = [164, 124, 80];
const PALLET_PINE: Rgb = [176, 134, 88];
const OLIVE: Rgb = [100, 108, 66];
const CAN_OLIVE: Rgb = [92, 102, 62];
const CHEST_OLIVE: Rgb = [84, 92, 60];
const CROSS_WHITE: Rgb = [214, 210, 196];
const CROSS_RED: Rgb = [164, 44, 38];
const FIBRE: Rgb = [52, 44, 36];
const ARMOUR: Rgb = [116, 120, 122];
const TAN: Rgb = [156, 138, 100];
const STEEL: Rgb = [70, 74, 78];
/** Stencil cream, as stores are marked, and black for the light tan case. */
const STENCIL_CREAM: Rgb = [222, 212, 178];
const STENCIL_BLACK: Rgb = [34, 36, 32];
/** Dust settled on the supply, the sheet a drop is laid on and the canvas an empty
 * pallet is bundled under. */
const DUST: Rgb = [148, 132, 106];
const CANVAS: Rgb = [92, 94, 70];
const BUNDLE_CANVAS: Rgb = [124, 116, 82];
/** The dark stencil sprayed across a restocking drop's stripe. */
const GLYPH_DARK: Rgb = [30, 30, 28];
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

/** Paint pressed into horizontal ribs below the band: a lit upper lip and a
 * shadowed lower one every `pitch` pixels. */
function ribbed(seed: number, base: Rgb) {
  const surface = paint(seed, base);
  const pitch = 38;
  return (x: number, y: number): Rgb => {
    const color = surface(x, y);
    if (y < BAND[1] + 8 || y > SIZE - 8) return color;
    const at = (y - BAND[1] - 8) % pitch;
    if (at < 3) return scale(color, 1.28);
    if (at < 6) return scale(color, 0.64);
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

function surfaceOf(surface: Surface, seed: number): (x: number, y: number) => Rgb {
  switch (surface) {
    case "planks":
      return planks(seed, PINE, 5);
    case "olive":
      return paint(seed, OLIVE);
    case "can":
      return ribbed(seed, CAN_OLIVE);
    case "fibre":
      return fibre(seed);
    case "steel":
      return armour(seed);
    case "jerry":
      return embossed(seed);
    case "drab":
      return ribbed(seed, CHEST_OLIVE);
    case "tan":
      return ribbed(seed, TAN);
  }
}

const luminance = ([r, g, b]: Rgb) => (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;

/** The stripe the stencil is sprayed across: the kind's paint, worn white behind the
 * medical chest's cross. */
function stripeOf(kind: PickupKind): Rgb {
  return kind === "repair" ? CROSS_WHITE : hex(PICKUP_PAINTS[kind]);
}

/** The stencil's paint: red for the cross, black on light paint, cream on dark. */
function stencilPaint(kind: PickupKind): Rgb {
  if (kind === "repair") return CROSS_RED;
  return luminance(stripeOf(kind)) > LIGHT_PAINT ? STENCIL_BLACK : STENCIL_CREAM;
}

/** A stripe of `color` brushed across `c` from edge to edge between `rows`: edges
 * that wander, ends worn back, the surface's seams and grain showing through the
 * paint, and chips knocked out where the noise runs high. */
function brushStripe(
  c: Context,
  rows: readonly [number, number],
  color: Rgb,
  seed: number,
  wear: number,
) {
  const image = c.getImageData(0, 0, SIZE, SIZE);
  const ragged = lattice(seed);
  const chips = lattice(seed + 3);
  // The surface's mean value across the stripe, so its relief is measured from it.
  let total = 0;
  for (let x = 0; x < SIZE; x++) {
    const i = (Math.round((rows[0] + rows[1]) / 2) * SIZE + x) * 4;
    total += luminance([image.data[i], image.data[i + 1], image.data[i + 2]]);
  }
  const level = Math.max(0.04, total / SIZE);
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      const top = rows[0] + STRIPE_RAGGED * (fbm(ragged, x * 0.05, 1.5, 0, 3) - 0.5) * 2;
      const bottom = rows[1] + STRIPE_RAGGED * (fbm(ragged, x * 0.05, 7.5, 0, 3) - 0.5) * 2;
      let cover = smooth(top - 1, top + 1, y) * (1 - smooth(bottom - 1, bottom + 1, y));
      if (cover <= 0) continue;
      // Worn back toward the ends of the stripe and chipped across it.
      const end = 1 - smooth(0, 30, Math.min(x, SIZE - x));
      const chip = fbm(chips, x * 0.12, y * 0.12, 0, 3) + 0.25 * end;
      cover *= 1 - smooth(0.76 - wear, 0.8 - wear, chip);
      const i = (y * SIZE + x) * 4;
      const under: Rgb = [image.data[i], image.data[i + 1], image.data[i + 2]];
      // The paint takes the surface's relief: seams and grain read through it.
      const relief = Math.min(1.12, Math.max(0.55, luminance(under) / level)) ** 0.6;
      const mottle = 0.92 + 0.12 * fbm(ragged, x * 0.03, y * 0.03, 0, 3);
      const grey = luminance(color) * 255;
      const enamel = blend(color, [grey, grey, grey], 0.12);
      const painted = scale(enamel, relief * mottle);
      image.data.set(
        blend(under, painted, cover).map((v) => Math.round(v)),
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
function sprayStencil(image: Image, ink: Rgb, at: Spray, size: number, seed: number) {
  const { stencil, bounds } = extractStencil(image, ink);
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

function craftTile(kind: PickupKind, image: Image) {
  const surface: Surface = PICKUP_SURFACES[kind];
  const tint = hex(PICKUP_PAINTS[kind]);
  const seed = 0x5a7e + PICKUP_PAINTS[kind];
  const canvas = createCanvas(SIZE, SIZE);
  const c = canvas.getContext("2d");
  fill(c, surfaceOf(surface, seed));
  // The band in the kind's paint across the top, worn back toward its ends.
  const band = createCanvas(SIZE, SIZE);
  const b = band.getContext("2d");
  b.fillStyle = css(tint);
  b.fillRect(0, BAND[0], SIZE, BAND[1] - BAND[0]);
  b.fillStyle = css(scale(tint, 0.6));
  b.fillRect(0, BAND[0], SIZE, 2);
  b.fillRect(0, BAND[1] - 2, SIZE, 2);
  weather(band, seed + 11, 0.06, (x) => 1 - smooth(0, 48, Math.min(x, SIZE - x)));
  c.drawImage(band, 0, 0);
  // The colour-code stripe brushed across the face, the stencil sprayed over it.
  brushStripe(c, STRIPE, stripeOf(kind), seed + 17, STRIPE_WEAR);
  c.drawImage(sprayStencil(image, stencilPaint(kind), TILE_SPRAY, SIZE, seed), 0, 0);
  settleDust(c, seed + 31);
  if (surface === "planks") cornerPlates(c);
  return canvas;
}

/** Dust and grime over a whole tile: drifts where the noise runs high, rain streaks
 * run down from the top edge, and dust banks up toward the foot of the side faces
 * (`SIDE_FOOT`) and the bottom of every face. */
function settleDust(c: Context, seed: number) {
  const image = c.getImageData(0, 0, SIZE, SIZE);
  const drift = lattice(seed);
  const rain = lattice(seed + 1);
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      const low = (y / SIZE) ** 3;
      const foot = smooth(SIDE_FOOT - 30, SIDE_FOOT, y) * (1 - smooth(SIDE_FOOT, SIDE_FOOT + 8, y));
      const amount =
        0.2 * smooth(0.5, 0.85, fbm(drift, x * 0.025, y * 0.025, 0, 4)) + 0.26 * low + 0.24 * foot;
      const streak =
        smooth(0.55, 0.75, fbm(rain, x * 0.12, y * 0.006, 0, 3)) *
        0.2 *
        (1 - smooth(SIDE_FOOT - 16, SIDE_FOOT, y));
      const i = (y * SIZE + x) * 4;
      for (let k = 0; k < 3; k++) {
        const dusty = image.data[i + k] + (DUST[k] - image.data[i + k]) * amount;
        image.data[i + k] = dusty * (1 - streak);
      }
    }
  }
  c.putImageData(image, 0, 0);
}

/** The nine stencils a restocking drop shows, a 3x3 block of small tiles laid out
 * like the kind tiles: a stripe of white paint brushed across the canvas, which the
 * marking tints in the kind's signal colour, and the pictogram sprayed dark across
 * it, so it reads on any colour. Transparent round the stripe. */
function glyphCell(images: Map<PickupKind, Image>) {
  const canvas = createCanvas(SIZE, SIZE);
  const c = canvas.getContext("2d");
  const tile = Math.floor(SIZE / 3);
  const edge = 4.5;
  for (const [kind, [column, row]] of Object.entries(PICKUP_ATLAS_TILES)) {
    const image = images.get(kind as PickupKind)!;
    const seed = 0x61f + column * 7 + row * 13;
    const stripe = createCanvas(tile, tile);
    const s = stripe.getContext("2d");
    // White paint on transparency, its brushed outline wandering and chipped.
    const paint = s.createImageData(tile, tile);
    const ragged = lattice(seed);
    const chips = lattice(seed + 5);
    const wander = (u: number, row: number) => edge * (fbm(ragged, u * 0.08, row, 0, 3) - 0.5) * 2;
    for (let y = 0; y < tile; y++) {
      for (let x = 0; x < tile; x++) {
        const t = tile * 0.14 + wander(x, 1.5);
        const b = tile * 0.86 + wander(x, 7.5);
        const l = edge + wander(y, 3.5);
        const r = tile - edge + wander(y, 9.5);
        let cover =
          smooth(t - 0.8, t + 0.8, y) *
          (1 - smooth(b - 0.8, b + 0.8, y)) *
          smooth(l - 0.8, l + 0.8, x) *
          (1 - smooth(r - 0.8, r + 0.8, x));
        cover *= 1 - smooth(0.7, 0.75, fbm(chips, x * 0.2, y * 0.2, 0, 3));
        // Sprayed on canvas: uneven coats, the weave and grime showing through.
        const weave = (Math.floor(x / 1.5) + Math.floor(y / 1.5)) % 2 ? 0.93 : 1;
        const coat = (0.72 + 0.28 * fbm(chips, x * 0.06 + 9.1, y * 0.06, 0, 3)) * weave;
        const value = 240 * coat;
        paint.data.set(
          [value, value * 0.99, value * 0.95, Math.round(255 * cover)],
          (y * tile + x) * 4,
        );
      }
    }
    s.putImageData(paint, 0, 0);
    const sprayed = sprayStencil(
      image,
      GLYPH_DARK,
      {
        center: [tile / 2, tile / 2],
        max: [tile * 0.7, tile * 0.58],
        spread: 1.5,
        blur: 2,
        bridge: 2,
      },
      tile,
      seed,
    );
    // The dark paint lands only on the stripe; its overspray greys the white.
    s.globalCompositeOperation = "source-atop";
    s.drawImage(sprayed, 0, 0);
    c.drawImage(stripe, column * tile, row * tile);
  }
  return canvas;
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

/** The canvas sheet a drop is laid on, seen from above: its weave, the creases it
 * was folded along, a stitched hem, dust blown onto its edges, and the shade under
 * the pallet standing on it (`PALLET_SIZE` on `TARP_SIZE` in `pickup_visuals.rs`),
 * deepest in the gaps between its stringers and fading out round its foot. */
function tarpCell() {
  const canvas = createCanvas(SIZE, SIZE);
  const tone = lattice(0x7a4b1);
  const dust = lattice(0x7a4b3);
  const hem = 10;
  const [left, top] = [SIZE * 0.1, SIZE * (0.1 / 1.4)];
  fill(canvas.getContext("2d"), (x, y) => {
    const weave = (Math.floor(x / 2) + Math.floor(y / 2)) % 2 ? 0.95 : 1.03;
    let k = weave * (0.86 + 0.28 * fbm(tone, x * 0.02, y * 0.02, 0, 4));
    // Fold creases: a lit ridge beside a dark valley, across and along.
    for (const crease of [Math.abs(x - SIZE / 2), Math.abs(y - SIZE * 0.36)]) {
      if (crease < 1.5) k *= 0.7;
      else if (crease < 4) k *= 1.12;
    }
    const near = edgeDistance(x, y);
    if (near < hem) k *= near < 2 ? 0.72 : near > hem - 2 ? 0.8 : 1.1;
    // Contact shade: the distance outside the pallet's footprint (negative under it).
    const outside = Math.max(left - x, x - (SIZE - left), top - y, y - (SIZE - top));
    k *= 1 - 0.5 * (1 - smooth(-2, 16, outside));
    const dusty = (1 - smooth(0, 40, near)) * 0.55 + 0.2 * fbm(dust, x * 0.03, y * 0.03, 0, 3);
    return blend(
      scale(CANVAS, k),
      scale(DUST, k),
      dusty * smooth(0.2, 0.9, fbm(dust, x * 0.06, y * 0.06, 0, 2)),
    );
  });
  return canvas;
}

/** Rows at the foot of the bundle cell holding its webbing (`BUNDLE_STRAP` in
 * `pickup_visuals.rs`). */
const BUNDLE_STRAP_ROWS = 32;

/** The khaki canvas an empty pallet is bundled under, seen from above and from its
 * sides alike: its weave, a seam stitched across it, slack folds where it is drawn
 * over the load, darker grime low down and dust settled on it; in its bottom rows the
 * olive webbing strapping it down. */
function bundleCell() {
  const canvas = createCanvas(SIZE, SIZE);
  const tone = lattice(0xb0d1);
  const folds = lattice(0xb0d3);
  const dust = lattice(0xb0d5);
  fill(canvas.getContext("2d"), (x, y) => {
    const weave = (Math.floor(x / 2) + Math.floor(y / 2)) % 2 ? 0.95 : 1.03;
    let k = weave * (0.84 + 0.3 * fbm(tone, x * 0.025, y * 0.025, 0, 4));
    // Slack folds running down from where the canvas is drawn tight: lit ridges and
    // shaded valleys.
    const fold = fbm(folds, x * 0.05, y * 0.012, 0, 3);
    k *= 0.78 + 0.4 * smooth(0.3, 0.7, fold);
    // A stitched seam across it.
    const seam = Math.abs(y - SIZE * 0.42);
    if (seam < 1.5) k *= 0.62;
    else if (seam < 3.5) k *= 1.1;
    else if (seam < 5 && Math.floor(x / 4) % 2) k *= 0.85;
    const grime = 0.35 * smooth(0.6, 1, y / SIZE) + 0.2 * fbm(dust, x * 0.04, y * 0.04, 0, 3);
    // The bottom rows: the olive webbing strapping it down.
    if (y >= SIZE - BUNDLE_STRAP_ROWS) {
      const twill = (Math.floor(x / 3) + Math.floor(y / 3)) % 2 ? 0.88 : 1.06;
      return scale([84, 88, 58], twill * (0.86 + 0.24 * fbm(tone, x * 0.08, y * 0.3, 0, 2)));
    }
    return blend(scale(BUNDLE_CANVAS, k), scale([72, 64, 50], k), grime);
  });
  return canvas;
}

/** The ground's spray paint, run along the cell and seamless from its left edge to
 * its right so it bends once round a drop: a wide, uneven pass of white over a dark
 * primer edge, its line wandering, thinner and broken where the can was lifted, a
 * second pass laid over part of it, overspray flecks beside it, and dirt, dust and
 * tread scuffs worn through it, so it reads as paint on the ground, not a drawn line. */
function paintCell() {
  const canvas = createCanvas(SIZE, SIZE);
  const c = canvas.getContext("2d");
  const image = c.createImageData(SIZE, SIZE);
  const wander = lattice(0x5b1a7);
  const breaks = lattice(0x5b1a9);
  const dirt = lattice(0x5b1ab);
  const specks = lattice(0x5b1ad);
  const second = lattice(0x5b1af);
  // Periods in lattice cells across the cell, so the stroke wraps seamlessly.
  const WANDER_PERIOD = 6;
  const BREAK_PERIOD = 7;
  const stroke = (u: number, v: number, centre: number, half: number, rough: number) =>
    1 - smooth(0.72, 1.08, Math.abs(v - centre) / half + (rough - 0.5) * 0.55);
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      const u = (x + 0.5) / SIZE;
      const v = (y + 0.5) / SIZE;
      const centre = 0.5 + 0.18 * (fbm(wander, u * WANDER_PERIOD, 0.5, WANDER_PERIOD, 3) - 0.5);
      // Where the can was lifted: the pass thins out and breaks off.
      const lift = fbm(breaks, u * BREAK_PERIOD, 3.5, BREAK_PERIOD, 2);
      const gap = smooth(0.24, 0.33, lift);
      const half = 0.17 + 0.17 * gap;
      const rough = fbm(wander, u * 64, v * 16, 64, 3);
      let coverage = stroke(u, v, centre, half, rough) * gap;
      // A second pass over part of the circle, a little off the first.
      const again = smooth(0.52, 0.62, fbm(second, u * 4, 1.5, 4, 2));
      const centre2 = centre + 0.1 * (fbm(second, u * 8, 5.5, 8, 2) - 0.5) * 2;
      coverage = Math.max(coverage, stroke(u, v, centre2, 0.16, rough) * again);
      // Dirt, dust and treads worn through the paint.
      const grime = fbm(dirt, u * 40, v * 5, 40, 4);
      coverage *= 1 - 0.85 * smooth(0.58, 0.7, grime);
      const scuff = fbm(dirt, u * 6, v * 1.2 + 9.5, 6, 2);
      coverage *= 1 - 0.6 * smooth(0.62, 0.75, scuff);
      // Overspray flecks beside the stroke.
      const across = Math.abs(v - centre) / half;
      const fleck = specks(u * 160, v * 70, 160);
      if (across > 0.9 && across < 2.4 && fleck > 0.92 - 0.06 * gap)
        coverage = Math.max(coverage, 0.55);
      // A dark primer edge round a light stroke, so the ring reads on pale sand and
      // on dark asphalt alike; dust settled over the paint.
      const dusty = 0.82 + 0.18 * fbm(dirt, u * 20, v * 3 + 4.5, 20, 3);
      const edge = Math.min(across, Math.abs(v - centre2) / 0.16 + (1 - again) * 9);
      const shade = (220 + 30 * rough) * dusty * (1 - 0.45 * smooth(0.6, 0.85, edge));
      image.data.set(
        [shade, shade, shade, Math.round(Math.min(1, coverage) * 255)],
        (y * SIZE + x) * 4,
      );
    }
  }
  c.putImageData(image, 0, 0);
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
const images = new Map<PickupKind, Image>();
for (const [kind, [column, row]] of Object.entries(PICKUP_ATLAS_TILES)) {
  const image = await loadImage(
    new URL(`../assets/texture-sources/pickups/${kind}.webp`, import.meta.url),
  );
  if (image.width !== SIZE || image.height !== SIZE) {
    throw new Error(`Unexpected pickup icon dimensions: ${kind}`);
  }
  images.set(kind as PickupKind, image);
  place(craftTile(kind as PickupKind, image), column, row);
}
place(glyphCell(images), ...PICKUP_GLYPH_CELL);
place(palletCell(), ...PICKUP_PALLET_CELL);
place(paintCell(), ...PICKUP_PAINT_CELL);
place(swatchCell(), ...PICKUP_SWATCH_CELL);
place(brassCell(), ...PICKUP_BRASS_CELL);
place(tarpCell(), ...PICKUP_TARP_CELL);
place(bundleCell(), ...PICKUP_BUNDLE_CELL);
const webp = execFileSync(
  "cwebp",
  ["-quiet", "-q", ENCODE_QUALITY, "-alpha_q", "100", "-m", "6", "-o", "-", "--", "-"],
  { input: atlas.toBuffer("image/png"), maxBuffer: 32 * 1024 * 1024 },
);
await writeFile(new URL(`../public/${PICKUP_ATLAS_PATH}`, import.meta.url), webp);
console.log(`public/${PICKUP_ATLAS_PATH}`);
