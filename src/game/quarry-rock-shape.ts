import { Random } from "./math";

/** Shared visible/collision surface: weathered octagonal outcrops, 70 triangles each. */
export function quarryRockShape(w: number, h: number, d: number, variant: number) {
  const rng = new Random(812 + variant);
  const outline = [
    [-0.62, -1],
    [0.51, -0.95],
    [1, -0.48],
    [0.94, 0.57],
    [0.48, 1],
    [-0.61, 0.91],
    [-1, 0.43],
    [-0.94, -0.54],
  ].map(([x, z]) => [x * rng.range(0.94, 1), z * rng.range(0.94, 1)]);
  const positions: number[] = [];
  for (const [level, scale] of [
    [0, 1],
    [0.37, 1],
    [0.44, 0.96],
    [0.78, 0.86],
    [1, 0.64],
  ]) {
    for (const [x, z] of outline) {
      const irregular = level > 0.45 ? rng.range(0.87, 1.08) : 1;
      positions.push(
        (x * w * scale * irregular) / 2,
        level * h + (level > 0.45 ? rng.range(-0.2, 0.16) : 0),
        (z * d * scale * irregular) / 2,
      );
    }
  }
  const indices: number[] = [];
  for (let ring = 0; ring < 4; ring++) {
    for (let side = 0; side < 8; side++) {
      const a = ring * 8 + side;
      const b = ring * 8 + ((side + 1) % 8);
      indices.push(a, a + 8, b, b, a + 8, b + 8);
    }
  }
  for (let i = 1; i < 7; i++) {
    indices.push(32, 32 + i + 1, 32 + i);
  }
  return { positions: new Float32Array(positions), indices: new Uint32Array(indices) };
}

export function quarryRockVariant(x: number, z: number): number {
  return Math.abs(Math.round(x + z)) % 4;
}
