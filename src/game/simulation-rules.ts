/** Physics and lifecycle tuning, in metres and seconds. Positive gravity points down. */
export const GRAVITY = 22;
export const MAX_FRAGMENTS = 80;
export const SIMULATION_RULES = {
  defaultSeed: 12345,
  defaultTankCount: 12,
  spawnProtectionSeconds: 2,
  respawnSeconds: 3,
  pickupRadius: 1.8,
  recoilRecoveryPerSecond: 6,
  maxPendingEvents: 400,
  // A new deterministic stream for each round; multiplication is reduced to uint32.
  roundSeedStride: 0x9e3779b9,
  tankBodyHeight: 0.65,
  tankLinearDamping: 0.35,
  tankAngularDamping: 8,
} as const;
export const SOLO = {
  activeEnemies: 6,
  enemyHealthMultiplier: 0.4,
  enemyDamageMultiplier: 0.4,
  reinforcementSeconds: 1,
  spawnX: 53,
  spawnHalfSpanZ: 46,
} as const;
export const SPAWN_SCORING = {
  maximumEnemyDistance: 50,
  visibleEnemyPenalty: 12,
  allyClearance: 5,
  allyProximityPenalty: 8,
} as const;
