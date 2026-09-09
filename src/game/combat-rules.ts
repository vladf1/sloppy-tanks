/** Shared combat distances are metres; durations are seconds. */
export const COMBAT = {
  projectileLifetime: 3.5,
  spreadAngle: 0.19,
  rocketBlastRadius: 5.3,
  rocketTopSpeedMultiplier: 2.5,
  rocketAccelerationSeconds: 1,
  rapidReloadMultiplier: 0.5,
  // Leave a gap after a ray contact so the next query does not hit the same face at time zero.
  muzzleClearance: 0.001,
  bounceClearance: 0.025,
  contactTimeEpsilon: 1e-8,
  separationEpsilon: 1e-6,
  // Each shell can bounce/intercept several times in a tick; cap repeated zero-time contacts.
  contactsPerShot: 8,
  minimumBlastDamageFraction: 0.25,
  blastImpulse: 9,
  minimumBlastDistance: 0.1,
  coverBlastAllowance: 0.35,
  drumBlastRadius: 6,
  drumDamage: 75,
} as const;

export const MINE = {
  damage: 100,
  armSeconds: 0.8,
  lifetimeSeconds: 25,
  cooldownSeconds: 7,
  triggerRadius: 2.5,
  blastRadius: 5.7,
} as const;
