import RAPIER from "@dimforge/rapier3d-compat";

/** Match the twelve-sided barrel: it rolls, then rests on a facet instead of creeping forever. */
export function barrelCollider(w: number, h: number, d: number): RAPIER.ColliderDesc {
  const points = new Float32Array(24 * 3);
  let at = 0;
  for (const y of [-h / 2, h / 2]) {
    for (let i = 0; i < 12; i++) {
      const angle = (i * Math.PI) / 6;
      points[at++] = (Math.sin(angle) * w) / 2;
      points[at++] = y;
      points[at++] = (Math.cos(angle) * d) / 2;
    }
  }
  return RAPIER.ColliderDesc.convexHull(points)!;
}
