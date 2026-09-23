import type { Simulation } from "./simulation";
import type { Fragment } from "./types";

/** The last second sinks and fades, without shrinking or blocking tanks. */
export const DEBRIS_CLEANUP_SECONDS = 1;

export function debrisMoving(fragment: Fragment): boolean {
  if (fragment.body.isSleeping()) {
    return false;
  }
  const v = fragment.body.linvel();
  const w = fragment.body.angvel();
  return v.x * v.x + v.y * v.y + v.z * v.z > 0.16 || w.x * w.x + w.y * w.y + w.z * w.z > 0.25;
}

/** Prefer distant settled pieces, then distant moving pieces, preserving nearby action. */
export function cleanupCandidate(
  sim: Simulation,
  candidates = sim.fragments,
): Fragment | undefined {
  const human = sim.tanks.find((tank) => tank.human);
  const focus = human?.alive ? human.body.translation() : (human?.previous ?? { x: 0, z: 0 });
  const players = sim.multiplayer
    ? sim.tanks
        .filter((tank) => tank.human)
        .map((tank) => (tank.alive ? tank.body.translation() : tank.previous))
    : undefined;
  let best: Fragment | undefined;
  let bestScore = Infinity;
  for (const fragment of candidates) {
    const p = fragment.body.translation();
    const distance2 = players?.length
      ? Math.min(...players.map((point) => (p.x - point.x) ** 2 + (p.z - point.z) ** 2))
      : (p.x - focus.x) ** 2 + (p.z - focus.z) ** 2;
    const priority = (distance2 < 25 ** 2 ? 2 : 0) + (debrisMoving(fragment) ? 1 : 0);
    // Discrete priority dominates; within it prefer already fading and older pieces.
    const score =
      priority * 1000 + Math.min(fragment.life, 100) - Math.min(distance2, 10000) * 0.00001;
    if (score < bestScore) {
      best = fragment;
      bestScore = score;
    }
  }
  return best;
}

/** Start the normal fade before the hard budget forces an immediate eviction. */
export function prepareDebrisCleanup(sim: Simulation): void {
  const target = Math.floor(sim.maxFragments * 0.8);
  if (sim.fragments.length <= target) {
    return;
  }
  let excess = sim.fragments.filter((f) => f.life > DEBRIS_CLEANUP_SECONDS).length - target;
  if (excess <= 0) {
    return;
  }
  const candidates = sim.fragments.filter(
    (f) => f.life > DEBRIS_CLEANUP_SECONDS && f.life <= 3 && !debrisMoving(f),
  );
  while (excess-- > 0 && candidates.length) {
    const fragment = cleanupCandidate(sim, candidates)!;
    fragment.life = DEBRIS_CLEANUP_SECONDS;
    candidates.splice(candidates.indexOf(fragment), 1);
  }
}

export function debrisCleanupProgress(life: number): number {
  const t = Math.max(0, Math.min(1, 1 - life / DEBRIS_CLEANUP_SECONDS));
  return t * t * (3 - 2 * t);
}
