export const DRAGON_TOOTH_SCALE = 0.9;
export const DRAGON_TOOTH_MASS = 9.6 * DRAGON_TOOTH_SCALE ** 3;

// Narrow lifting crowns and pointed castings, with imperfect field orientation.
const TOOTH_PROFILES = [
  { yaw: -0.24, topScale: 0.12 },
  { yaw: 0.34, topScale: 0 },
  { yaw: -0.1, topScale: 0.16 },
  { yaw: 0.46, topScale: 0.1 },
] as const;

export function dragonToothVariant(x: number, z: number): number {
  return Math.round(Math.abs(x) * 23 + Math.abs(z) * 37) % TOOTH_PROFILES.length;
}

export function dragonToothProfile(variant: number) {
  return TOOTH_PROFILES[variant % TOOTH_PROFILES.length];
}

/** Unit-box coordinates become the same rotated pyramid in rendering and physics.
 * Fit the rotation inside w/d so navigation retains the exact ground footprint. */
export function dragonToothPoint(
  x: number,
  y: number,
  z: number,
  w: number,
  h: number,
  d: number,
  variant: number,
) {
  const { yaw, topScale } = dragonToothProfile(variant);
  const cos = Math.cos(yaw);
  const sin = Math.sin(yaw);
  const scale = (1 - (1 - topScale) * (y + 0.5)) / (Math.abs(cos) + Math.abs(sin));
  return [(x * cos + z * sin) * w * scale, y * h, (z * cos - x * sin) * d * scale];
}
export const HEDGEHOG_BEAMS = [
  { length: 3.1, rx: 0, rz: Math.PI / 4 },
  { length: 3.1, rx: 0, rz: -Math.PI / 4 },
  { length: 3.1, rx: Math.PI / 2, rz: 0 },
] as const;

/** Convex hull points matching sloping concrete and open steel shapes, so shots pass
 * through visible gaps. Each steel flange/web is convex, preserving the open gaps on a
 * dynamic body. Plain points keep the physics engine out of rendering's imports. */
export function quarryBarrierHulls(
  kind: "teeth" | "hedgehog",
  w: number,
  h: number,
  d: number,
  variant = 0,
) {
  if (kind === "teeth") {
    const points: number[] = [];
    for (const y of [-0.5, 0.5]) {
      for (const x of [-0.5, 0.5]) {
        for (const z of [-0.5, 0.5]) {
          points.push(...dragonToothPoint(x, y, z, w, h, d, variant));
        }
      }
    }
    return [new Float32Array(points)];
  }
  const hulls: Float32Array[] = [];
  for (const beam of HEDGEHOG_BEAMS) {
    for (const [offset, width, depth] of [
      [0, 0.12, 0.44],
      [-0.22, 0.1, 0.52],
      [0.22, 0.1, 0.52],
    ]) {
      const vertices: number[] = [];
      for (const z of [-depth / 2, depth / 2]) {
        for (const y of [-beam.length / 2, beam.length / 2]) {
          for (const x of [-width / 2, width / 2]) {
            const bx = x + offset;
            const by = y * Math.cos(beam.rx) - z * Math.sin(beam.rx);
            const bz = y * Math.sin(beam.rx) + z * Math.cos(beam.rx);
            vertices.push(
              ((bx * Math.cos(beam.rz) - by * Math.sin(beam.rz)) * w) / 2.9,
              ((bx * Math.sin(beam.rz) + by * Math.cos(beam.rz) + 1.3) * h) / 2.7 - h / 2,
              (bz * d) / 3.2,
            );
          }
        }
      }
      hulls.push(new Float32Array(vertices));
    }
  }
  return hulls;
}
