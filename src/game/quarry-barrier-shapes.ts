import RAPIER from "@dimforge/rapier3d-compat";

export const TOOTH_TOP_SCALE = 0.3;
export const HEDGEHOG_BEAMS = [
  { length: 3.1, rx: 0, rz: Math.PI / 4 },
  { length: 3.1, rx: 0, rz: -Math.PI / 4 },
  { length: 3.1, rx: Math.PI / 2, rz: 0 },
] as const;

/** Match sloping concrete and open steel shapes, so shots pass through visible gaps.
 * Navigation still uses the full tank-blocking footprint. Static trimeshes cost no dynamics. */
export function quarryBarrierCollider(kind: "teeth" | "hedgehog", w: number, h: number, d: number) {
  if (kind === "teeth") {
    const points: number[] = [];
    for (const y of [-h / 2, h / 2]) {
      const scale = y > 0 ? TOOTH_TOP_SCALE : 1;
      for (const x of [-1, 1]) {
        for (const z of [-1, 1]) {
          points.push((x * w * scale) / 2, y, (z * d * scale) / 2);
        }
      }
    }
    return RAPIER.ColliderDesc.convexHull(new Float32Array(points))!;
  }
  const vertices: number[] = [];
  const indices: number[] = [];
  for (const beam of HEDGEHOG_BEAMS) {
    for (const [offset, width, depth] of [
      [0, 0.12, 0.44],
      [-0.22, 0.1, 0.52],
      [0.22, 0.1, 0.52],
    ]) {
      const first = vertices.length / 3;
      for (const z of [-depth / 2, depth / 2]) {
        for (const y of [-beam.length / 2, beam.length / 2]) {
          for (const x of [-width / 2, width / 2]) {
            const bx = x + offset;
            const by = y * Math.cos(beam.rx) - z * Math.sin(beam.rx);
            const bz = y * Math.sin(beam.rx) + z * Math.cos(beam.rx);
            vertices.push(
              bx * Math.cos(beam.rz) - by * Math.sin(beam.rz),
              bx * Math.sin(beam.rz) + by * Math.cos(beam.rz) + 1.3 - h / 2,
              bz,
            );
          }
        }
      }
      for (const index of [
        0, 2, 1, 1, 2, 3, 4, 5, 6, 5, 7, 6, 0, 1, 4, 1, 5, 4, 2, 6, 3, 3, 6, 7, 0, 4, 2, 2, 4, 6, 1,
        3, 5, 3, 7, 5,
      ]) {
        indices.push(first + index);
      }
    }
  }
  return RAPIER.ColliderDesc.trimesh(new Float32Array(vertices), new Uint32Array(indices));
}
