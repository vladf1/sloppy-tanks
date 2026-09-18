import type { Cover } from "./types";

export const TIMBER_HEALTH = 80;

/** Ends use the increasing world X/Z axis, independent of the model's yaw. */
export interface TimberJoin {
  openMin?: boolean;
  openMax?: boolean;
  post?: boolean;
}

export interface TimberHit {
  x: number;
  y: number;
  z: number;
  size: number;
}
export interface TimberMark {
  x: number;
  y: number;
  face: "front" | "back" | "left" | "right";
  size: number;
  seed: number;
}

export interface TimberPart {
  kind: "beam" | "post";
  index: number;
  x: number;
  y: number;
  z: number;
  w: number;
  h: number;
  d: number;
  yaw: number;
  lean: number;
  color: number;
  damage: number;
  damageSeed: number;
  marks: TimberMark[];
}

export function timberDamageStage(hp: number, maxHp: number): number {
  return hp >= maxHp ? 0 : hp > maxHp * 0.5 ? 1 : hp > maxHp * 0.2 ? 2 : 3;
}

const BEAM_COUNT = 4;

/** The same members are used by the standing wall and its physical debris. */
export function timberParts(
  c: Pick<Cover, "w" | "h" | "d" | "color" | "timberHits" | "timberJoin"> &
    Partial<Pick<Cover, "x" | "z">>,
  stage: number,
): TimberPart[] {
  const along = c.w > c.d;
  const length = Math.max(c.w, c.d);
  const depth = Math.min(c.w, c.d);
  const yaw = along ? 0 : Math.PI / 2;
  const postWidth = Math.min(0.32, length * 0.12);
  // Separate physical members must start clear of the posts, even in the loosened pose.
  const endClearance = 0.04 + c.h * 0.02;
  const join = c.timberJoin;
  const openNegative = along ? join?.openMin : join?.openMax;
  const openPositive = along ? join?.openMax : join?.openMin;
  const inset = 0.18 + postWidth / 2 + endClearance;
  const beamMin = -length / 2 + (openNegative ? 0.025 : inset);
  const beamMax = length / 2 - (openPositive ? 0.025 : inset);
  const beamCenter = (beamMin + beamMax) / 2;
  const pitch = (c.h - 0.12) / BEAM_COUNT;
  const colors = [c.color, 0x94613e, 0xa66f46];
  // Cosmetic randomness is stable per wall and never consumes the combat RNG.
  const seed =
    Math.imul(Math.round((c.x ?? 0) * 100), 73856093) ^
    Math.imul(Math.round((c.z ?? 0) * 100), 19349663);
  const parts: TimberPart[] = [];
  for (let index = 0; index < (join?.post ? 0 : BEAM_COUNT); index++) {
    const damage = index === 1 ? stage : Math.max(0, stage - (index === 2 ? 1 : 2));
    parts.push({
      kind: "beam",
      index,
      x: along ? beamCenter : 0,
      y: 0.06 + pitch * (index + 0.5),
      z: along ? 0 : -beamCenter,
      w: beamMax - beamMin,
      h: pitch - 0.025,
      d: Math.min(0.66, depth * 0.75),
      yaw,
      lean: index === BEAM_COUNT - 1 ? stage * 0.006 : 0,
      color: colors[index % colors.length],
      damage,
      marks: [],
      damageSeed: seed ^ Math.imul(index + 1, 83492791),
    });
  }
  for (const side of join?.post ? [0] : [-1, 1]) {
    if ((side < 0 && openNegative) || (side > 0 && openPositive)) {
      continue;
    }
    const offset = side * (length / 2 - 0.18);
    parts.push({
      kind: "post",
      index: side === 0 ? 0 : side < 0 ? BEAM_COUNT : BEAM_COUNT + 1,
      x: along ? offset : 0,
      y: c.h / 2,
      z: along ? 0 : -offset,
      w: join?.post ? length : postWidth,
      h: c.h,
      d: depth,
      yaw,
      lean: stage === 3 ? side * 0.028 : 0,
      color: 0x805336,
      damage: stage,
      marks: [],
      damageSeed: seed ^ Math.imul(side + 7, 83492791),
    });
  }
  // Attach each hit to its nearest actual beam/post and only the struck face.
  for (const [hitIndex, hit] of (c.timberHits ?? []).entries()) {
    const candidates = parts
      .map((part) => {
        const dx = hit.x - part.x;
        const dz = hit.z - part.z;
        const x = dx * Math.cos(yaw) - dz * Math.sin(yaw);
        const z = dx * Math.sin(yaw) + dz * Math.cos(yaw);
        const y = hit.y - part.y;
        const distance =
          Math.max(0, Math.abs(x) - part.w / 2) ** 2 +
          Math.max(0, Math.abs(y) - part.h / 2) ** 2 +
          Math.max(0, Math.abs(z) - part.d / 2) ** 2;
        return { part, x, y, z, distance };
      })
      .sort((a, b) => a.distance - b.distance);
    const { part, x, y, z } = candidates[0];
    const end = Math.abs(x) > part.w / 2 - 0.015;
    part.marks.push({
      x: end ? z : x,
      y,
      face: end ? (x < 0 ? "left" : "right") : z < 0 ? "back" : "front",
      size: hit.size,
      seed: part.damageSeed ^ Math.imul(hitIndex + 1, 0x45d9f3b),
    });
  }
  // Marks attach in the original piece coordinates, then move with a loosened beam.
  const shift = stage >= 2 && !openNegative && !openPositive ? 0.02 * (stage - 1) : 0;
  if (!join?.post) {
    parts[BEAM_COUNT - 1].x += along ? shift : 0;
    parts[BEAM_COUNT - 1].z -= along ? 0 : shift;
  }
  return parts;
}
