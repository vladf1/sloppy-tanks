import { readFileSync, writeFileSync } from "node:fs";

// Bakes Battle Setup's logo, SLOPPY over TANKS, into path data for the desktop shell's
// icon rasterizer (`LOGO_SLOPPY` and `LOGO_TANKS` in `crates/desktop/src/ui/icons.rs`).
// The page drew the words in Impact, which may not be redistributed; these are Anton's
// outlines (SIL OFL, `assets/fonts/`), each letter stretched into the box Impact's took
// in the page's `.setup-brand`, so the logo keeps its old shape on every platform.
// Run `node --import tsx scripts/generate-logo-outlines.ts` after changing the fit.

const FONT = new URL("../assets/fonts/Anton-Regular.ttf", import.meta.url);
const TARGET = new URL("../crates/desktop/src/ui/icons.rs", import.meta.url);

/** A word's letters and the ink box of each: left, top, right, bottom. */
interface Word {
  constant: string;
  text: string;
  letters: [number, number, number, number][];
}

// Impact's letters in the page's layout box, in CSS pixels: 27 and 41 px on 0.82
// lines, 0.3 px letter spacing and Impact's kerning, placed (to a tenth of a pixel)
// where the 3x capture of the production page that the logo used to be shows them.
const WORDS: Word[] = [
  {
    constant: "LOGO_SLOPPY",
    text: "SLOPPY",
    letters: [
      [0.49, 0.26, 13.18, 22.5],
      [15.22, 0.7, 24.14, 22.05],
      [25.22, 0.26, 38.06, 22.5],
      [40.43, 0.7, 52.39, 22.05],
      [54.28, 0.7, 66.24, 22.05],
      [66.87, 0.7, 79.96, 22.05],
    ],
  },
  {
    constant: "LOGO_TANKS",
    text: "TANKS",
    letters: [
      [0.07, 22.67, 18.49, 55.08],
      [17.15, 22.67, 38.45, 55.08],
      [40.19, 22.67, 59.01, 55.08],
      [62.69, 22.67, 83.23, 55.08],
      [83.67, 21.99, 102.93, 55.76],
    ],
  },
];

/** Outlines grown outwards by this share of a letter's height, for Impact's weight. */
const EMBOLDEN = 0.0075;

interface Point {
  x: number;
  y: number;
  on: boolean;
}

interface Glyph {
  contours: Point[][];
  /** Ink box: left, bottom, right, top (font units, y up). */
  box: [number, number, number, number];
}

/** The glyph outlines of the TrueType font at `url`, by character. */
function readFont(url: URL): (char: string) => Glyph {
  const bytes = readFileSync(url);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const tables = new Map<string, number>();
  for (let index = 0; index < view.getUint16(4); index++) {
    const record = 12 + index * 16;
    tables.set(bytes.toString("latin1", record, record + 4), view.getUint32(record + 8));
  }
  const table = (tag: string) => {
    const offset = tables.get(tag);
    if (offset === undefined) throw new Error(`the font has no ${tag} table`);
    return offset;
  };
  const longOffsets = view.getInt16(table("head") + 50) === 1;
  const loca = table("loca");
  const glyf = table("glyf");

  // Unicode BMP (platform 3, encoding 1) in cmap format 4.
  const cmap = table("cmap");
  let format4 = -1;
  for (let index = 0; index < view.getUint16(cmap + 2); index++) {
    const record = cmap + 4 + index * 8;
    const subtable = cmap + view.getUint32(record + 4);
    if (
      view.getUint16(record) === 3 &&
      view.getUint16(record + 2) === 1 &&
      view.getUint16(subtable) === 4
    ) {
      format4 = subtable;
    }
  }
  if (format4 < 0) throw new Error("the font has no Unicode cmap");
  const glyphIndex = (code: number) => {
    const segments = view.getUint16(format4 + 6) / 2;
    const ends = format4 + 14;
    const starts = ends + 2 * segments + 2;
    const deltas = starts + 2 * segments;
    const ranges = deltas + 2 * segments;
    for (let segment = 0; segment < segments; segment++) {
      if (view.getUint16(ends + 2 * segment) < code) continue;
      const start = view.getUint16(starts + 2 * segment);
      if (start > code) break;
      const delta = view.getInt16(deltas + 2 * segment);
      const range = view.getUint16(ranges + 2 * segment);
      if (range === 0) return (code + delta) & 0xffff;
      const index = view.getUint16(ranges + 2 * segment + range + 2 * (code - start));
      return index === 0 ? 0 : (index + delta) & 0xffff;
    }
    throw new Error(`the font has no glyph for U+${code.toString(16)}`);
  };

  return (char: string) => {
    const index = glyphIndex(char.codePointAt(0)!);
    const at = (i: number) =>
      longOffsets ? view.getUint32(loca + 4 * i) : 2 * view.getUint16(loca + 2 * i);
    let offset = glyf + at(index);
    const contourCount = view.getInt16(offset);
    if (contourCount <= 0) throw new Error(`${char} is not a simple glyph`);
    const box: Glyph["box"] = [
      view.getInt16(offset + 2),
      view.getInt16(offset + 4),
      view.getInt16(offset + 6),
      view.getInt16(offset + 8),
    ];
    offset += 10;
    const ends: number[] = [];
    for (let contour = 0; contour < contourCount; contour++, offset += 2)
      ends.push(view.getUint16(offset));
    offset += 2 + view.getUint16(offset);
    const pointCount = ends[ends.length - 1] + 1;
    const flags: number[] = [];
    while (flags.length < pointCount) {
      const flag = view.getUint8(offset++);
      flags.push(flag);
      if (flag & 8)
        for (let repeat = view.getUint8(offset++); repeat > 0; repeat--) flags.push(flag);
    }
    // Coordinates are deltas: a short one with its sign in a flag, or a word, or none.
    const coordinates = (short: number, same: number) => {
      let value = 0;
      return flags.map((flag) => {
        if (flag & short) value += (flag & same ? 1 : -1) * view.getUint8(offset++);
        else if (!(flag & same)) {
          value += view.getInt16(offset);
          offset += 2;
        }
        return value;
      });
    };
    const xs = coordinates(2, 0x10);
    const ys = coordinates(4, 0x20);
    const contours: Point[][] = [];
    let first = 0;
    for (const end of ends) {
      const contour: Point[] = [];
      for (let point = first; point <= end; point++) {
        contour.push({ x: xs[point], y: ys[point], on: (flags[point] & 1) === 1 });
      }
      contours.push(contour);
      first = end + 1;
    }
    return { contours, box };
  };
}

/** `contour` grown `amount` outwards, FreeType's way: each point moves along the
 * bisector of its edges' normals, far enough to move both edges by `amount` (limited at
 * sharp corners). TrueType winds outer contours clockwise with y up, so the ink is on
 * each edge's right and its outside on the left. */
function embolden(contour: Point[], amount: number): Point[] {
  const count = contour.length;
  const neighbour = (index: number, step: number) => {
    for (let k = 1; k < count; k++) {
      const other = contour[(index + step * k + count) % count];
      if (other.x !== contour[index].x || other.y !== contour[index].y) return other;
    }
    return contour[index];
  };
  return contour.map((point, index) => {
    const previous = neighbour(index, -1);
    const next = neighbour(index, 1);
    const normal = (from: Point, to: Point) => {
      const length = Math.hypot(to.x - from.x, to.y - from.y);
      return [-(to.y - from.y) / length, (to.x - from.x) / length];
    };
    const [ax, ay] = normal(previous, point);
    const [bx, by] = normal(point, next);
    const bisector = Math.hypot(ax + bx, ay + by);
    if (bisector < 1e-6) return point;
    const scale = Math.min(amount / (1 + ax * bx + ay * by), (2 * amount) / bisector);
    return { x: point.x + (ax + bx) * scale, y: point.y + (ay + by) * scale, on: point.on };
  });
}

/** The path data of one closed TrueType contour (quadratic, with implied on-curve
 * points between two off-curve ones), rounded to hundredths of a pixel. */
function contourPath(contour: Point[]): string {
  const middle = (a: Point, b: Point): Point => ({
    x: (a.x + b.x) / 2,
    y: (a.y + b.y) / 2,
    on: true,
  });
  const firstOn = contour.findIndex((point) => point.on);
  const start = firstOn < 0 ? middle(contour[contour.length - 1], contour[0]) : contour[firstOn];
  const rest =
    firstOn < 0 ? contour : [...contour.slice(firstOn + 1), ...contour.slice(0, firstOn)];
  const hundredths = (point: Point) => [Math.round(point.x * 100), Math.round(point.y * 100)];
  let [x, y] = hundredths(start);
  let text = `M${numbers([x, y])}`;
  let command = "M";
  // Steps relative to the current point, between rounded points so rounding never
  // accumulates; straight ones along an axis are `h` or `v`, and a repeated command
  // is implied.
  const step = (kind: "l" | "q", points: Point[]) => {
    const deltas = points.flatMap((point) => {
      const [toX, toY] = hundredths(point);
      return [toX - x, toY - y];
    });
    [x, y] = hundredths(points[points.length - 1]);
    let [next, values] = [kind as string, deltas];
    if (kind === "l" && deltas[1] === 0) [next, values] = ["h", [deltas[0]]];
    else if (kind === "l" && deltas[0] === 0) [next, values] = ["v", [deltas[1]]];
    text += (next === command ? (values[0] < 0 ? "" : " ") : next) + numbers(values);
    command = next;
  };
  let control: Point | null = null;
  const points = [...rest, start];
  points.forEach((point, index) => {
    if (point.on) {
      if (control) step("q", [control, point]);
      else if (index < points.length - 1) step("l", [point]);
      control = null;
    } else {
      if (control) step("q", [control, middle(control, point)]);
      control = point;
    }
  });
  return `${text}z`;
}

/** Hundredths as SVG numbers, separated by a space or by a minus sign. */
function numbers(values: number[]): string {
  return values
    .map((value, index) => (index > 0 && value >= 0 ? " " : "") + String(value / 100))
    .join("");
}

const glyphs = readFont(FONT);
let source = readFileSync(TARGET, "utf8");
for (const word of WORDS) {
  const strings = [...word.text].map((char, index) => {
    const glyph = glyphs(char);
    const [left, top, right, bottom] = word.letters[index];
    const [inkLeft, inkBottom, inkRight, inkTop] = glyph.box;
    // The grown outline fills the box: stretched to it less the growth on each side.
    const grow = EMBOLDEN * (bottom - top);
    const horizontal = (right - left - 2 * grow) / (inkRight - inkLeft);
    const vertical = (bottom - top - 2 * grow) / (inkTop - inkBottom);
    const path = glyph.contours
      .map((contour) => {
        const scaled = contour.map((point) => ({
          x: point.x * horizontal,
          y: point.y * vertical,
          on: point.on,
        }));
        // Into the layout box, y down.
        const placed = embolden(scaled, grow).map((point) => ({
          x: left + grow + point.x - inkLeft * horizontal,
          y: bottom - grow - (point.y - inkBottom * vertical),
          on: point.on,
        }));
        return contourPath(placed);
      })
      .join("");
    return `    // ${char}\n    "${path}",\n`;
  });
  const pattern = new RegExp(`(const ${word.constant}: &str = concat!\\(\n)[\\s\\S]*?(\\);)`);
  if (!pattern.test(source)) throw new Error(`icons.rs has no ${word.constant}`);
  source = source.replace(
    pattern,
    (_, head: string, tail: string) => `${head}${strings.join("")}${tail}`,
  );
}
writeFileSync(TARGET, source);
