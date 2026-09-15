/** The last second sinks and fades, without shrinking or blocking tanks. */
export const DEBRIS_CLEANUP_SECONDS = 1;

export function debrisCleanupProgress(life: number): number {
  const t = Math.max(0, Math.min(1, 1 - life / DEBRIS_CLEANUP_SECONDS));
  return t * t * (3 - 2 * t);
}
