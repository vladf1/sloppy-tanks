import type { PickupKind, VehicleKind, Weapon } from "./types";
export const STEP = 1 / 60,
  ARENA = 60,
  ROUND_TIME = 300,
  SOLO_TIME = 600,
  SCORE_LIMIT = 100;
const BASE_SPEED_MULTIPLIER = 1.13;
// Simulation distances use world metres; the selector rounds speeds to km/h.
// V-Tanks 570bf8d: Vanguard 184, standard shell 535; preserve that dodge
// ratio at our existing 19.2 m/s shell speed, then apply chassis ratios
// and the tank-only 20% increase, followed by the shared 13% pacing increase.
const referenceSpeed = (multiplier: number) => {
  const speed = (19.2 * 184 / 535) * multiplier * 1.2 * BASE_SPEED_MULTIPLIER;
  return { speed, speedKmh: Math.round(speed * 3.6) };
};
export const MOVE_ACCELERATION = 100;
export const HULL_TURN_SPEED = 9;
export const PLAYER_FIRE_RATE_MULTIPLIER = 1.2;
export const SHIELD_CAPACITY = 120; // Three standard 40-damage shells.
export const INTERCEPTION_RADIUS = 0.8;
export const INTERCEPTION_BLAST_RADIUS = 3;
export const MINE_RADIUS = 0.5;

export const VEHICLES: Record<
  VehicleKind,
  {
    name: string;
    tag: string;
    health: number;
    speed: number;
    speedKmh: number;
    mass: number;
    scale: number;
  }
> = {
  scout: {
    name: "SKIPPER",
    tag: "Light scout",
    health: 80,
    ...referenceSpeed(1.24),
    mass: 1,
    // Comparable game sizes; Bruiser anchors the fleet at 1.95 units wide.
    scale: (3.59 / 2.3) * (1.95 / 3.66),
  },
  balanced: {
    name: "BRUISER",
    tag: "Balanced tank",
    health: 100,
    ...referenceSpeed(1),
    mass: 1.45,
    scale: 1.95 / 2.42,
  },
  heavy: {
    name: "BIG RIG",
    tag: "Heavy tank",
    health: 140,
    ...referenceSpeed(0.76),
    mass: 2.5,
    scale: (3.5 / 2.5) * (1.95 / 3.66),
  },
};
export const WEAPONS: Record<
  Weapon,
  {
    name: string;
    interval: number;
    damage: number;
    speed: number;
    bounces: number;
    color: number;
    label: string;
    unit: string;
    perCrate: number;
    carryLimit: number;
  }
> = {
  standard: {
    label: "STANDARD", unit: "SHELLS", perCrate: 0, carryLimit: Infinity,
    name: "Standard shells",
    interval: 0.85,
    damage: 40,
    speed: 19.2 * BASE_SPEED_MULTIPLIER,
    bounces: 1,
    color: 0xffdf00,
  },
  spread: {
    label: "SPREAD", unit: "SPREAD VOLLEYS", perCrate: 18, carryLimit: 36,
    name: "Spread shot",
    interval: 1.1,
    damage: 27,
    speed: 17.6 * BASE_SPEED_MULTIPLIER,
    bounces: 1,
    color: 0xff38d4,
  },
  rocket: {
    label: "ROCKET", unit: "ROCKETS", perCrate: 12, carryLimit: 24,
    name: "Breaching rockets",
    interval: 1.3,
    damage: 65,
    speed: 13.6 * BASE_SPEED_MULTIPLIER,
    bounces: 0,
    color: 0xff591c,
  },
  ricochet: {
    name: "Ricochet shells", label: "RICOCHET", unit: "RICOCHET SHELLS",
    perCrate: 24, carryLimit: 48, interval: 0.85, damage: 80,
    speed: 19.2 * BASE_SPEED_MULTIPLIER, bounces: 3, color: 0xb19afc,
  },
  piercing: {
    name: "Piercing shells", label: "PIERCING", unit: "PIERCING SHELLS",
    perCrate: 24, carryLimit: 48, interval: 0.85, damage: 40,
    speed: 19.2 * BASE_SPEED_MULTIPLIER, bounces: 0, color: 0x54e6dc,
  },
};
export const LASER_DEFENSE = { chance: 0.5, duration: 6, range: 7, threatRadius: 3,
  initialDelay: 25, respawn: 45 } as const;
export const PICKUPS: Record<
  PickupKind,
  { name: string; icon: string; color: number; duration: number }
> = {
  rapid: { name: "RAPID FIRE", icon: "»", color: 0xffcf54, duration: 12 },
  spread: { name: "SPREAD AMMO", icon: "⋔", color: WEAPONS.spread.color, duration: 0 },
  rocket: { name: "ROCKET AMMO", icon: "↑", color: WEAPONS.rocket.color, duration: 0 },
  ricochet: { name: "RICOCHET AMMO", icon: "↗", color: WEAPONS.ricochet.color, duration: 0 },
  piercing: { name: "PIERCING AMMO", icon: "↟", color: WEAPONS.piercing.color, duration: 0 },
  shield: { name: "SHIELD", icon: "◇", color: 0x72dbef, duration: 15 },
  speed: { name: "SPEED BOOST", icon: "ϟ", color: 0xbbe574, duration: 12 },
  repair: { name: "REPAIR", icon: "+", color: 0x88ddb0, duration: 0 },
  laser: { name: "LASER DEFENSE", icon: "✧", color: 0x7bfff2, duration: LASER_DEFENSE.duration },
};
export const TEAM_COLORS = [0x008cff, 0xff303e];
export const TEAM_NAMES = ["BLUE", "RED"];
export const GROUP = {
  coverQuery: 0xffff0002, // Query all memberships, accepting cover only.
  steeringQuery: 0xffff0012, // Cover and tank-contact hulls, excluding cosmetic debris.
  tank: 0x00010006, // Model-sized hull touches cover and ground only.
  tankContact: 0x00100010, // Model-sized hulls touch other tank hulls only.
  cover: 0x0002000b,
  ground: 0x00040009,
  fragment: 0x00080006,
};
export class Random {
  constructor(public state: number) {}
  next() {
    let t = (this.state += 0x6d2b79f5);
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }
  range(a: number, b: number) {
    return a + (b - a) * this.next();
  }
}
export const distance = (
  a: { x: number; z: number },
  b: { x: number; z: number },
) => Math.hypot(a.x - b.x, a.z - b.z);
export const angleDelta = (a: number, b: number) =>
  Math.atan2(Math.sin(b - a), Math.cos(b - a));

/** Highest score wins; equal scores retain the original candidate order. */
export function bestBy<T>(items: Iterable<T>, score: (item: T) => number): T | undefined {
  let best: T | undefined, highest = -Infinity;
  for (const item of items) {
    const value = score(item);
    if (value > highest) { best = item; highest = value; }
  }
  return best;
}
