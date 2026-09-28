import { writeFile } from "node:fs/promises";

// Draws public/favicon.svg: the blue Bruiser in isometric projection with a navy outline.
// Flat faces are culled, flat-shaded polygons; round parts are projected ellipses and arcs
// so the file stays small. Elements are painted in the order they are added.

type Vec3 = [number, number, number];
type Vec2 = [number, number];
type Rgb = Vec3;
type Axis = "x" | "y" | "z";

const COS30 = Math.cos(Math.PI / 6);
const SIN30 = Math.sin(Math.PI / 6);
const project = ([x, y, z]: Vec3): Vec2 => [(x - z) * COS30, (x + z) * SIN30 - y];

const add = (...vectors: Vec3[]): Vec3 =>
  vectors.reduce((sum, v) => [sum[0] + v[0], sum[1] + v[1], sum[2] + v[2]]);
const scale = (v: Vec3, k: number): Vec3 => [v[0] * k, v[1] * k, v[2] * k];
const subtract = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
const normalize = (v: Vec3) => scale(v, 1 / Math.hypot(...v));
const centroid = (points: Vec3[]) => scale(add(...points), 1 / points.length);

/** Exact view direction of `project`. */
const EYE = normalize([1, 1, 1]);
/** Slightly raised culling direction keeps thin top faces that are nearly edge-on. */
const CULL_VIEW = normalize([1, 1.15, 1]);
const LIGHT = normalize([-0.3, 1, 0.8]);
const OUTLINE = "#0d1b2a";
const OUTLINE_WIDTH = 2.4;
const PADDING = 1.6;
const ICON_SIZE = 64;

const hex = (rgb: number[]) =>
  "#" +
  rgb
    .map((c) =>
      Math.max(0, Math.min(255, Math.round(c)))
        .toString(16)
        .padStart(2, "0"),
    )
    .join("");
const tint = (rgb: Rgb, k: number): Rgb => scale(rgb, k);
const shade = (rgb: Rgb, normal: Vec3) =>
  hex(tint(rgb, 0.5 + 0.62 * Math.max(0, dot(normal, LIGHT))));

/** In-plane U, V and the extrusion direction for each axis. */
const AXES: Record<Axis, [Vec3, Vec3, Vec3]> = {
  x: [
    [0, 0, 1],
    [0, 1, 0],
    [1, 0, 0],
  ],
  y: [
    [1, 0, 0],
    [0, 0, 1],
    [0, 1, 0],
  ],
  z: [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ],
};
const at = (axis: Axis, u: number, v: number, along: number) => {
  const [U, V, A] = AXES[axis];
  return add(scale(U, u), scale(V, v), scale(A, along));
};

type PathBuilder = (
  point: (p: Vec3) => string,
  arc: (U: Vec3, V: Vec3, radius: number, direction: 1 | -1, to: Vec3) => string,
) => string;
type Element =
  | { kind: "polygon"; points: Vec3[]; fill: string }
  | { kind: "hull"; points: Vec3[]; fill: string }
  | { kind: "disc"; center: Vec3; U: Vec3; V: Vec3; radius: number; fill: string }
  | { kind: "path"; build: PathBuilder; samples: Vec3[]; fill: string };
type Gradient = { axis: Vec3; from: Vec3; to: Vec3; stops: { at: Vec3; color: string }[] };

// Elements hold 3D data; they are written once the fitted scale is known.
const elements: Element[] = [];
const gradients: Gradient[] = [];
const polygon = (points: Vec3[], fill: string) => elements.push({ kind: "polygon", points, fill });
/** Circle of `radius` at `center` in the plane spanned by U and V. */
const disc = (center: Vec3, U: Vec3, V: Vec3, radius: number, fill: string) =>
  elements.push({ kind: "disc", center, U, V, radius, fill });

function prism(
  profile: Vec2[],
  axis: Axis,
  start: number,
  end: number,
  sideColor: (index: number) => Rgb,
  capColor: Rgb,
) {
  const cap0 = profile.map(([u, v]) => at(axis, u, v, start));
  const cap1 = profile.map(([u, v]) => at(axis, u, v, end));
  const faces = [
    { points: cap1, color: capColor },
    { points: [...cap0].reverse(), color: capColor },
  ];
  profile.forEach((_, i) => {
    const j = (i + 1) % profile.length;
    faces.push({ points: [cap0[i], cap0[j], cap1[j], cap1[i]], color: sideColor(i) });
  });
  const center = centroid([...cap0, ...cap1]);
  for (const face of faces) {
    const [a, b, c] = face.points;
    let normal = normalize(cross(subtract(b, a), subtract(c, a)));
    if (dot(normal, subtract(centroid(face.points), center)) < 0) normal = scale(normal, -1);
    if (dot(normal, CULL_VIEW) > 1e-6) polygon(face.points, shade(face.color, normal));
  }
}

/**
 * Cylinder along an axis. The side and its far rim are one gradient-filled path sampled from the
 * lighting across the visible half; the flat near end is drawn on top.
 */
function cylinder(
  axis: Axis,
  u: number,
  v: number,
  radius: number,
  start: number,
  end: number,
  color: Rgb,
  boreRadius?: number,
) {
  const [U, V, A] = AXES[axis];
  const ring = (t: number, along: number) =>
    add(at(axis, u, v, along), scale(U, radius * Math.cos(t)), scale(V, radius * Math.sin(t)));
  const normal = (t: number) => add(scale(U, Math.cos(t)), scale(V, Math.sin(t)));
  const [far, near] = dot(A, EYE) > 0 ? [start, end] : [end, start];
  const silhouette = Math.atan2(-dot(U, EYE), dot(V, EYE));
  const visible =
    dot(normal(silhouette + Math.PI / 2), EYE) > 0 ? silhouette : silhouette + Math.PI;
  const id = `g${gradients.length}`;
  gradients.push({
    axis: A,
    from: ring(visible, far),
    to: ring(visible + Math.PI, far),
    stops: Array.from({ length: 7 }, (_, i) => {
      const t = visible + (i / 6) * Math.PI;
      return { at: ring(t, far), color: shade(tint(color, 0.82), normal(t)) };
    }),
  });
  // One path for the side and its far rim: two abutting shapes would leave an anti-aliased seam.
  elements.push({
    kind: "path",
    build: (point, arc) =>
      `M${point(ring(visible, near))} ${point(ring(visible, far))}` +
      arc(U, V, radius, 1, ring(visible + Math.PI, far)) +
      `L${point(ring(visible + Math.PI, near))}Z`,
    samples: [
      ...Array.from({ length: 9 }, (_, i) => ring(visible + (i / 8) * Math.PI, far)),
      ring(visible, near),
      ring(visible + Math.PI, near),
    ],
    fill: `url(#${id})`,
  });
  // The flat end stays brighter than any part of the side so its rim reads as an edge.
  disc(at(axis, u, v, near), U, V, radius, hex(tint(color, 1.04)));
  if (boreRadius)
    disc(at(axis, u, v, near + 0.01 * Math.sign(near - far)), U, V, boreRadius, "#0b1622");
}

const solid = (color: Rgb) => () => color;
const BLUE: Rgb = [30, 144, 255];
const BLUE_LIGHT: Rgb = [90, 190, 255];
const TURRET_RING: Rgb = [16, 36, 64];
const TREAD: Rgb = [70, 86, 106];
const TREAD_DARK: Rgb = [44, 56, 72];
const STEEL: Rgb = [168, 192, 208];
const WHEEL: Rgb = [120, 140, 158];
const HUB: Rgb = [210, 226, 236];

// Tracks are a stadium extruded along x: z runs along the track and y is up.
const TRACK_LENGTH = 5.8;
const TRACK_RADIUS = 0.62;
const TRACK_CENTER_Y = 0.62;
const TRACK_HALF = TRACK_LENGTH / 2 - TRACK_RADIUS;
const TREAD_LUGS = 9;
const ROAD_WHEELS = 4;

function stadium(lugs: number, arcSteps: number) {
  const points: Vec2[] = [];
  const end = (centerZ: number, from: number) => {
    for (let i = 0; i <= arcSteps; i++) {
      const t = from + (i / arcSteps) * Math.PI;
      points.push([
        centerZ + TRACK_RADIUS * Math.cos(t),
        TRACK_CENTER_Y + TRACK_RADIUS * Math.sin(t),
      ]);
    }
  };
  end(TRACK_HALF, -Math.PI / 2);
  for (let i = 1; i < lugs; i++)
    points.push([TRACK_HALF - (2 * TRACK_HALF * i) / lugs, TRACK_CENTER_Y + TRACK_RADIUS]);
  end(-TRACK_HALF, Math.PI / 2);
  for (let i = 1; i < lugs; i++)
    points.push([-TRACK_HALF + (2 * TRACK_HALF * i) / lugs, TRACK_CENTER_Y - TRACK_RADIUS]);
  return points;
}

function convexHull(points: Vec2[]) {
  const sorted = [...points].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const turn = (o: Vec2, a: Vec2, b: Vec2) =>
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  const chain = (list: Vec2[]) =>
    list.reduce<Vec2[]>((hull, q) => {
      while (hull.length > 1 && turn(hull[hull.length - 2], hull[hull.length - 1], q) <= 0)
        hull.pop();
      hull.push(q);
      return hull;
    }, []);
  const lower = chain(sorted);
  const upper = chain([...sorted].reverse());
  return [...lower.slice(0, -1), ...upper.slice(0, -1)];
}

/** The belt is the hull of both caps with lighter lugs on alternate visible strips. */
function track(innerX: number, outerX: number, showOuterSide: boolean) {
  const profile = stadium(TREAD_LUGS, 6);
  elements.push({
    kind: "hull",
    points: profile.flatMap(([z, y]): Vec3[] => [
      [innerX, y, z],
      [outerX, y, z],
    ]),
    fill: shade(TREAD_DARK, [0, 1, 0]),
  });
  profile.forEach(([z0, y0], i) => {
    const [z1, y1] = profile[(i + 1) % profile.length];
    const normal = normalize([0, -(z1 - z0), y1 - y0]);
    if (i % 2 === 0 && dot(normal, CULL_VIEW) > 1e-6) {
      const strip: Vec3[] = [
        [innerX, y0, z0],
        [innerX, y1, z1],
        [outerX, y1, z1],
        [outerX, y0, z0],
      ];
      polygon(strip, shade(TREAD, normal));
    }
  });
  if (!showOuterSide) return;

  const [U, V] = AXES.x;
  const side = (z: number, y: number): Vec3 => [outerX, y, z];
  const top = TRACK_CENTER_Y + TRACK_RADIUS;
  const bottom = TRACK_CENTER_Y - TRACK_RADIUS;
  elements.push({
    kind: "path",
    build: (point, arc) =>
      `M${point(side(-TRACK_HALF, top))} ${point(side(TRACK_HALF, top))}` +
      arc(U, V, TRACK_RADIUS, -1, side(TRACK_HALF, bottom)) +
      `L${point(side(-TRACK_HALF, bottom))}` +
      arc(U, V, TRACK_RADIUS, -1, side(-TRACK_HALF, top)) +
      "Z",
    samples: stadium(1, 8).map(([z, y]) => side(z, y)),
    fill: shade(TREAD_DARK, [1, 0, 0]),
  });
  for (let k = 0; k < ROAD_WHEELS; k++) {
    const z = -TRACK_HALF + (2 * TRACK_HALF * k) / (ROAD_WHEELS - 1);
    disc(side(z, TRACK_CENTER_Y), U, V, 0.5, hex(tint(WHEEL, 0.72)));
    disc(side(z + 0.04, TRACK_CENTER_Y + 0.04), U, V, 0.4, hex(tint(WHEEL, 0.95)));
    disc(side(z + 0.05, TRACK_CENTER_Y + 0.05), U, V, 0.15, hex(HUB));
  }
}

const lightFront = (index: number) => (index === 2 ? BLUE_LIGHT : BLUE);
// Back to front, so later elements cover earlier ones.
track(-2.45, -1.4, false);
prism(
  [
    [-2.4, 0.7],
    [2.3, 0.7],
    [2.75, 1.2],
    [1.8, 1.85],
    [-2.3, 1.85],
    [-2.6, 1.3],
  ],
  "x",
  -1.75,
  1.75,
  lightFront,
  BLUE,
);
track(1.4, 2.45, true);
// Dark turret ring so the turret front separates from the similarly lit hull deck.
prism(
  [
    [-1.9, 1.85],
    [1.35, 1.85],
    [1.3, 2.05],
    [-1.9, 2.05],
  ],
  "x",
  -1.5,
  1.5,
  solid(TURRET_RING),
  TURRET_RING,
);
prism(
  [
    [-1.8, 2.05],
    [1.2, 2.05],
    [0.8, 3.25],
    [-1.5, 3.25],
  ],
  "x",
  -1.4,
  1.4,
  lightFront,
  BLUE,
);
cylinder("y", -0.3, -0.75, 0.55, 3.25, 3.45, STEEL); // hatch
cylinder("z", 0, 2.45, 0.62, 0.95, 1.5, STEEL); // mantlet
cylinder("z", 0, 2.45, 0.3, 1.5, 3.4, STEEL); // barrel
cylinder("z", 0, 2.45, 0.6, 3.3, 4.35, STEEL, 0.34); // oversized muzzle with a dark bore

// Fit the drawing into the icon box.
const discSamples = (center: Vec3, U: Vec3, V: Vec3, radius: number) =>
  Array.from({ length: 16 }, (_, i) =>
    add(
      center,
      scale(U, radius * Math.cos((i * Math.PI) / 8)),
      scale(V, radius * Math.sin((i * Math.PI) / 8)),
    ),
  );
const extent = elements
  .flatMap((e) =>
    e.kind === "disc"
      ? discSamples(e.center, e.U, e.V, e.radius)
      : e.kind === "path"
        ? e.samples
        : e.points,
  )
  .map(project);
const xs = extent.map((p) => p[0]);
const ys = extent.map((p) => p[1]);
const [minX, maxX, minY, maxY] = [
  Math.min(...xs),
  Math.max(...xs),
  Math.min(...ys),
  Math.max(...ys),
];
const inner = ICON_SIZE - 2 * PADDING;
const zoom = inner / Math.max(maxX - minX, maxY - minY);
const offsetX = PADDING + (inner - (maxX - minX) * zoom) / 2 - minX * zoom;
const offsetY = PADDING + (inner - (maxY - minY) * zoom) / 2 - minY * zoom;

/** One decimal without a leading zero keeps path data short. */
const format = (value: number) => String(+value.toFixed(1)).replace(/^(-?)0\./, "$1.");
const toScreen = (p: Vec3): Vec2 => {
  const [x, y] = project(p);
  return [x * zoom + offsetX, y * zoom + offsetY];
};
const point = (p: Vec3) => toScreen(p).map(format).join(" ");

/** Radii and rotation of the projected circle r·(U cos t + V sin t), and whether t runs counter-clockwise on screen. */
function ellipse(U: Vec3, V: Vec3, radius: number) {
  const [a, c] = project(U).map((v) => v * zoom * radius);
  const [b, d] = project(V).map((v) => v * zoom * radius);
  const [p, q, s] = [a * a + b * b, a * c + b * d, c * c + d * d];
  const mid = (p + s) / 2;
  const deviation = Math.hypot((p - s) / 2, q);
  return {
    rx: Math.sqrt(mid + deviation),
    ry: Math.sqrt(Math.max(0, mid - deviation)),
    angle: (Math.atan2(2 * q, p - s) / 2) * (180 / Math.PI),
    counterClockwise: a * d - b * c < 0,
  };
}

/** Half-turn arc to `to`; direction -1 walks t downward. */
const arc = (U: Vec3, V: Vec3, radius: number, direction: 1 | -1, to: Vec3) => {
  const e = ellipse(U, V, radius);
  const sweep = direction < 0 === e.counterClockwise ? 1 : 0;
  return `A${format(e.rx)} ${format(e.ry)} ${format(e.angle)} 0 ${sweep} ${point(to)}`;
};
const polygonPath = (points: Vec2[]) =>
  "M" + points.map((p) => p.map(format).join(" ")).join(" ") + "Z";

const shapes = elements.map((e) => {
  if (e.kind === "polygon")
    return `<path d="${polygonPath(e.points.map(toScreen))}" fill="${e.fill}"/>`;
  if (e.kind === "hull")
    return `<path d="${polygonPath(convexHull(e.points.map(toScreen)))}" fill="${e.fill}"/>`;
  if (e.kind === "path") return `<path d="${e.build(point, arc)}" fill="${e.fill}"/>`;
  const { rx, ry, angle } = ellipse(e.U, e.V, e.radius);
  const [cx, cy] = toScreen(e.center).map(format);
  return `<ellipse cx="${cx}" cy="${cy}" rx="${format(rx)}" ry="${format(ry)}" transform="rotate(${format(angle)} ${cx} ${cy})" fill="${e.fill}"/>`;
});

// Shading is constant along a cylinder, so the gradient runs perpendicular to its projected axis.
const gradientDefs = gradients
  .map((g, i) => {
    const [ax, ay] = project(g.axis);
    const length = Math.hypot(ax, ay);
    let [dx, dy] = [-ay / length, ax / length];
    const [x1, y1] = toScreen(g.from);
    const [bx, by] = toScreen(g.to);
    const width = (bx - x1) * dx + (by - y1) * dy;
    [dx, dy] = [dx * Math.sign(width), dy * Math.sign(width)];
    const span = Math.abs(width);
    const stops = g.stops
      .map(({ at: p, color }) => {
        const [x, y] = toScreen(p);
        const offset = +(((x - x1) * dx + (y - y1) * dy) / span).toFixed(2);
        return `<stop offset="${offset}" stop-color="${color}"/>`;
      })
      .join("");
    return `<linearGradient id="g${i}" gradientUnits="userSpaceOnUse" x1="${format(x1)}" y1="${format(y1)}" x2="${format(x1 + dx * span)}" y2="${format(y1 + dy * span)}">${stops}</linearGradient>\n`;
  })
  .join("");

// The outline is the same shapes stroked underneath, so reuse them instead of repeating the paths.
const svg = `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 ${ICON_SIZE} ${ICON_SIZE}">
<title>Sloppy Tanks — blue Bruiser</title>
<defs>${gradientDefs}<g id="t">
${shapes.join("\n")}
</g></defs>
<use href="#t" xlink:href="#t" stroke="${OUTLINE}" stroke-width="${OUTLINE_WIDTH}" stroke-linejoin="round"/>
<use href="#t" xlink:href="#t"/>
</svg>
`;
await writeFile(new URL("../public/favicon.svg", import.meta.url), svg);
