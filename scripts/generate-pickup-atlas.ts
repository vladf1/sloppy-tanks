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
  PICKUP_PAINT_CELL,
  PICKUP_PAINTS,
  PICKUP_PALLET_CELL,
  PICKUP_SURFACES,
  PICKUP_SWATCH_CELL,
} from "./asset-data";
import { fbm, lattice, smooth } from "./texture-noise";

// Paint the pickups as military supply. Each kind's tile is the surface its supply
// shows (weathered pine, olive drab, black fibre, rolled armour steel, pressed jerry
// can steel, a ribbed chest, a tan transit case), with a band of the kind's paint
// across the top and the source pictogram sprayed large below it as a stencil, on a
// dark painted plate where the surface is too light to carry it. Wear gathers where
// hands, straps and the ground touch: chipped paint and grime along the edges, worn
// band ends, clean faces in between. The fourth column holds the pallet's bare
// planks, the spray paint marking the ground (white, its strokes, gaps and overspray
// in alpha, seamless across the cell so it bends once round a drop) and swatches of
// gunmetal, rubber, webbing and bright steel; below the block, cartridge brass. Edge
// pixels are extruded into every gutter so filtering and mipmaps never sample a
// neighbour. Encoded lossy: the surfaces are noise, which lossless packs badly.

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
 * plate and the stencil's centre and largest extent below them. */
const CORNER = 26;
const BAND = [26, 62] as const;
const PLATE = { left: 30, top: 98, right: 226, bottom: 240, radius: 12 } as const;
const SYMBOL_CENTER = [128, 168] as const;
const SYMBOL_MAX = [150, 128] as const;
/** How far the stencil's strokes are spread, so thin lines carry from above. */
const STROKE_SPREAD = 4;
/** How far in from a face's edge its paint chips and grime gathers. */
const EDGE_WEAR = 22;
/** How deep into a face its paint chips. */
const CHIP_DEPTH = 12;
const PINE: Rgb = [164, 124, 80];
const PALLET_PINE: Rgb = [178, 144, 100];
const OLIVE: Rgb = [88, 96, 60];
const CAN_OLIVE: Rgb = [80, 90, 56];
const CASE_GREEN: Rgb = [80, 90, 68];
const FIBRE: Rgb = [52, 44, 36];
const ARMOUR: Rgb = [116, 120, 122];
const TAN: Rgb = [156, 138, 100];
const STEEL: Rgb = [70, 74, 78];
const PLATE_PAINT: Rgb = [38, 40, 34];
const WHITE_PLATE: Rgb = [226, 228, 220];
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
    if (chip > 0.8 - 0.22 * rim) return scale(STEEL, 0.75 + 0.3 * k);
    if (chip > 0.77 - 0.22 * rim) return scale(base, 1.3 * k);
    const grime = edge * 0.22 + 0.06 * fbm(dust, x * 0.02, y * 0.02, 0, 3);
    return blend(scale(base, k), [112, 104, 88], grime);
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
    case "ribbed":
      return ribbed(seed, CASE_GREEN);
    case "tan":
      return ribbed(seed, TAN);
  }
}

/** The plate a stencil is sprayed on, where the surface itself is too light or busy
 * to carry the kind's paint from above. */
function plateOf(kind: PickupKind, surface: Surface): Rgb | null {
  if (kind === "repair") return WHITE_PLATE;
  return surface === "planks" || surface === "tan" ? PLATE_PAINT : null;
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
) {
  const c = canvas.getContext("2d");
  const image = c.getImageData(0, 0, SIZE, SIZE);
  const noise = lattice(seed);
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      const wear = fbm(noise, x * 0.09, y * 0.09, 0, 3) + 0.3 * edge(x, y);
      const i = (y * SIZE + x) * 4 + 3;
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

function roundedRect(
  c: Context,
  left: number,
  top: number,
  right: number,
  bottom: number,
  r: number,
) {
  c.beginPath();
  c.moveTo(left + r, top);
  c.arcTo(right, top, right, bottom, r);
  c.arcTo(right, bottom, left, bottom, r);
  c.arcTo(left, bottom, left, top, r);
  c.arcTo(left, top, right, top, r);
  c.closePath();
}

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
  weather(band, seed + 11, 0.04, (x) => 1 - smooth(0, 40, Math.min(x, SIZE - x)));
  c.drawImage(band, 0, 0);
  // The plate, painted by hand: soft-cornered, its rim worn through in places.
  const plate = plateOf(kind, surface);
  if (plate) {
    const layer = createCanvas(SIZE, SIZE);
    const p = layer.getContext("2d");
    p.fillStyle = css(plate);
    roundedRect(p, PLATE.left, PLATE.top, PLATE.right, PLATE.bottom, PLATE.radius);
    p.fill();
    weather(
      layer,
      seed + 17,
      0.02,
      (x, y) =>
        1 -
        smooth(0, 10, Math.min(x - PLATE.left, PLATE.right - x, y - PLATE.top, PLATE.bottom - y)),
    );
    c.drawImage(layer, 0, 0);
  }
  // The stencil, sprayed: a faint overspray halo, then solid paint, frayed a
  // little at its fringe.
  const { stencil, bounds } = extractStencil(image, tint);
  const fit = Math.min(SYMBOL_MAX[0] / bounds.width, SYMBOL_MAX[1] / bounds.height);
  const [width, height] = [bounds.width * fit, bounds.height * fit];
  const sprayed = createCanvas(SIZE, SIZE);
  const s = sprayed.getContext("2d");
  const spray = (alpha: number, blur: number, spread: number) => {
    s.globalAlpha = alpha;
    s.shadowColor = blur ? css(tint, 0.9) : "transparent";
    s.shadowBlur = blur;
    for (let step = 0; step < 12; step++) {
      const angle = (step / 12) * Math.PI * 2;
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
  spray(0.05, 5, STROKE_SPREAD);
  spray(1, 0, STROKE_SPREAD);
  weather(sprayed, seed + 23, 0.0, () => 0);
  c.drawImage(sprayed, 0, 0);
  if (surface === "planks") cornerPlates(c);
  return canvas;
}

function palletCell() {
  const canvas = createCanvas(SIZE, SIZE);
  fill(canvas.getContext("2d"), planks(0x9a11e7, PALLET_PINE, 4));
  return canvas;
}

/** White spray paint run along the cell, seamless from its left edge to its right:
 * a stroke whose edges wander, thinner and broken where the can was lifted,
 * flecked with overspray beside it and dirt through it. */
function paintCell() {
  const canvas = createCanvas(SIZE, SIZE);
  const c = canvas.getContext("2d");
  const image = c.createImageData(SIZE, SIZE);
  const wander = lattice(0x5b1a7);
  const breaks = lattice(0x5b1a9);
  const dirt = lattice(0x5b1ab);
  const specks = lattice(0x5b1ad);
  // Periods in lattice cells across the cell, so the stroke wraps seamlessly.
  const WANDER_PERIOD = 8;
  const BREAK_PERIOD = 5;
  for (let y = 0; y < SIZE; y++) {
    for (let x = 0; x < SIZE; x++) {
      const u = x + 0.5;
      const v = (y + 0.5) / SIZE;
      const centre =
        0.5 + 0.12 * (fbm(wander, (u / SIZE) * WANDER_PERIOD, 0.5, WANDER_PERIOD, 3) - 0.5);
      const lifted = smooth(
        0.17,
        0.27,
        fbm(breaks, (u / SIZE) * BREAK_PERIOD, 3.5, BREAK_PERIOD, 2),
      );
      const half = 0.12 + 0.2 * lifted;
      const across = Math.abs(v - centre) / half;
      const rough = fbm(wander, (u / SIZE) * 64, v * 16, 64, 3);
      let coverage = (1 - smooth(0.7, 1.15, across + (rough - 0.5) * 0.6)) * (0.35 + 0.65 * lifted);
      // Dirt and tread wear through the paint.
      const grime = fbm(dirt, (u / SIZE) * 32, v * 6, 32, 4);
      coverage *= 1 - smooth(0.6, 0.72, grime);
      // Overspray flecks beside the stroke.
      const fleck = specks((u / SIZE) * 160, v * 70, 160);
      if (across > 0.9 && across < 2.2 && fleck > 0.9 - 0.1 * lifted)
        coverage = Math.max(coverage, 0.6);
      const shade = 214 + 36 * rough;
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

/** A cartridge from base to tip, the same all round: a dark rim, the brass case
 * darkening toward its shoulder, the copper driving band and the olive projectile. */
function brassCell() {
  const canvas = createCanvas(SIZE, SIZE);
  const streak = lattice(0xb4a55);
  fill(canvas.getContext("2d"), (x, y) => {
    const v = 1 - y / SIZE;
    const n = fbm(streak, x * 0.08, y * 0.01, 0, 3);
    if (v < 0.05) return scale([118, 88, 42], 0.9 + 0.2 * n);
    if (v < 0.6) {
      const shoulder = smooth(0.45, 0.6, v);
      return scale(blend([224, 178, 86], [176, 128, 56], shoulder), 0.88 + 0.26 * n);
    }
    if (v < 0.66) return scale([164, 92, 52], 0.9 + 0.2 * n);
    if (v > 0.9) return scale([214, 186, 64], 0.9 + 0.2 * n);
    return scale([112, 118, 88], 0.88 + 0.24 * n);
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
place(paintCell(), ...PICKUP_PAINT_CELL);
place(swatchCell(), ...PICKUP_SWATCH_CELL);
place(brassCell(), ...PICKUP_BRASS_CELL);
const webp = execFileSync(
  "cwebp",
  ["-quiet", "-q", ENCODE_QUALITY, "-alpha_q", "100", "-m", "6", "-o", "-", "--", "-"],
  { input: atlas.toBuffer("image/png"), maxBuffer: 32 * 1024 * 1024 },
);
await writeFile(new URL(`../public/${PICKUP_ATLAS_PATH}`, import.meta.url), webp);
console.log(`public/${PICKUP_ATLAS_PATH}`);
