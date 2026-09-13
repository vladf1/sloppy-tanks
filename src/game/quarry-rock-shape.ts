import { Random } from "./math";

/** Shared render/collision mesh: broken sediment shelves and an uneven eroded cap. */
export function quarryRockShape(w: number, h: number, d: number, variant: number) {
  const rng = new Random(812 + variant);
  const corners = [
    [-0.62, -1],
    [0.51, -0.95],
    [1, -0.48],
    [0.94, 0.57],
    [0.48, 1],
    [-0.61, 0.91],
    [-1, 0.43],
    [-0.94, -0.54],
  ].map(([x, z]) => [x * rng.range(0.89, 1), z * rng.range(0.89, 1)]);
  const outline = corners.flatMap((p, i) => {
    const next = corners[(i + 1) % corners.length];
    const split = rng.range(0.35, 0.65);
    const inset = rng.range(0.91, 1.02);
    return [
      p,
      [(p[0] + (next[0] - p[0]) * split) * inset, (p[1] + (next[1] - p[1]) * split) * inset],
    ];
  });
  const tiltX = rng.range(-0.11, 0.11);
  const tiltZ = rng.range(-0.09, 0.09);
  const cornerHeights = corners.map(() => rng.range(0.87, 0.98));
  const crowns = outline.map((_, i) =>
    i % 2 === 0
      ? cornerHeights[i / 2]
      : (cornerHeights[Math.floor(i / 2)] + cornerHeights[(Math.floor(i / 2) + 1) % 8]) / 2,
  );
  const erosion = outline.map(() => rng.range(-0.025, 0.025));
  const rings = [
    [0, 1],
    [0.07, 1],
    [0.28, 0.96],
    [0.44, 0.94],
    [0.72, 0.89],
    [0.88, 0.84],
    [1, 0.68],
  ];
  const positions: number[] = [];
  for (const [level, scale] of rings) {
    for (let side = 0; side < outline.length; side++) {
      const [x, z] = outline[side];
      // Local erosion breaks a few faces, without wrapping every rock in identical steps.
      const wornScale = scale + Math.sin(level * 8 + side * 0.7) * erosion[side] * level;

      positions.push(
        (x * w * wornScale) / 2,
        level === 0 ? -0.12 : h * level * (crowns[side] + x * tiltX + z * tiltZ),
        (z * d * wornScale) / 2,
      );
    }
  }
  const count = outline.length;
  const indices: number[] = [];
  for (let ring = 0; ring < rings.length - 1; ring++) {
    for (let side = 0; side < count; side++) {
      const a = ring * count + side;
      const b = ring * count + ((side + 1) % count);
      indices.push(a, a + count, b, b, a + count, b + count);
    }
  }
  const center = positions.length / 3;
  positions.push(w * rng.range(-0.08, 0.08), h * 0.96, d * rng.range(-0.08, 0.08));
  const cap = (rings.length - 1) * count;
  for (let i = 0; i < count; i++) {
    indices.push(center, cap + ((i + 1) % count), cap + i);
  }
  return { positions: new Float32Array(positions), indices: new Uint32Array(indices) };
}

export function quarryRockVariant(x: number, z: number): number {
  return Math.abs(Math.round(x * 17 + z * 31)) % 97;
}
