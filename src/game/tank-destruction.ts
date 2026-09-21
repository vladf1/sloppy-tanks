/** Stable cosmetic/aftermath choice without consuming the combat RNG stream. */
function destructionHash(seed: number, id: number, deaths: number): number {
  let hash = (seed ^ Math.imul(id, 0x9e3779b1) ^ Math.imul(deaths, 0x85ebca6b)) >>> 0;
  hash = Math.imul(hash ^ (hash >>> 16), 0x7feb352d);
  hash = Math.imul(hash ^ (hash >>> 15), 0x846ca68b);
  hash = (hash ^ (hash >>> 16)) >>> 0;
  return hash;
}

export function tankBurnout(seed: number, id: number, deaths: number): boolean {
  return destructionHash(seed, id, deaths) % 5 === 0;
}

export function humveeTumble(seed: number, id: number, deaths: number) {
  const hash = destructionHash(seed ^ 0x51ed270b, id, deaths);
  const direction = hash & 4 ? 1 : -1;
  // Local axes: pitch across the chassis, roll along its length.
  const profiles = [
    { height: 0.7, pitch: 0, roll: 5.5, yaw: 0, damping: 1.1 },
    { height: 1.5, pitch: 0.6, roll: 6, yaw: 0.5, damping: 0.65 },
    { height: 2.6, pitch: 4.7, roll: 0.7, yaw: 0.4, damping: 0.5 },
    { height: 2, pitch: 2.8, roll: 3.8, yaw: 1.4, damping: 0.65 },
  ];
  const profile = profiles[hash % profiles.length];
  return {
    ...profile,
    pitch: profile.pitch * direction,
    roll: profile.roll * direction,
    yaw: profile.yaw * direction,
  };
}
