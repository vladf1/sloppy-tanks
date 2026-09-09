/** Gameplay balance uses metres, seconds, radians and hit points unless stated otherwise. */
export { angleDelta, bestBy, distance, Random } from "./math";
import type { PickupKind, VehicleKind, Weapon } from "./types";
export const STEP = 1 / 60;
export const ARENA = 60;
export const ROUND_TIME = 300;
export const SOLO_TIME = 600;
export const SCORE_LIMIT = 100;
const BASE_SPEED_MULTIPLIER = 1.13;
const BASE_STANDARD_SHELL_SPEED = 19.2;
const REFERENCE_TANK_SPEED = 184;
const REFERENCE_SHELL_SPEED = 535;
const TANK_PACING_MULTIPLIER = 1.2;
export const KMH_PER_METRE_PER_SECOND = 3.6;
// Simulation distances use world metres; the selector rounds speeds to km/h.
// V-Tanks 570bf8d: Vanguard 184, standard shell 535; preserve that dodge
// ratio at our existing 19.2 m/s shell speed, then apply chassis ratios
// and the tank-only 20% increase, followed by the shared 13% pacing increase.
const referenceSpeed = (multiplier: number) => {
  const speed =
    ((BASE_STANDARD_SHELL_SPEED * REFERENCE_TANK_SPEED) / REFERENCE_SHELL_SPEED) *
    multiplier *
    TANK_PACING_MULTIPLIER *
    BASE_SPEED_MULTIPLIER;
  return { speed, speedKmh: Math.round(speed * KMH_PER_METRE_PER_SECOND) };
};
export const MOVE_ACCELERATION = 100;
export const HULL_TURN_SPEED = 3.5; // A quarter turn takes about 0.45 seconds.
export const REVERSE_SPEED = 0.8;
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
    label: "STANDARD",
    unit: "SHELLS",
    perCrate: 0,
    carryLimit: Infinity,
    name: "Standard shells",
    interval: 0.85,
    damage: 40,
    speed: BASE_STANDARD_SHELL_SPEED * BASE_SPEED_MULTIPLIER,
    bounces: 1,
    color: 0xffdf00,
  },
  spread: {
    label: "SPREAD",
    unit: "SPREAD VOLLEYS",
    perCrate: 18,
    carryLimit: 36,
    name: "Spread shot",
    interval: 1.1,
    damage: 27,
    speed: 17.6 * BASE_SPEED_MULTIPLIER,
    bounces: 1,
    color: 0xff38d4,
  },
  rocket: {
    label: "ROCKET",
    unit: "ROCKETS",
    perCrate: 12,
    carryLimit: 24,
    name: "Breaching rockets",
    interval: 1.3,
    damage: 65,
    speed: 13.6 * BASE_SPEED_MULTIPLIER,
    bounces: 0,
    color: 0xff591c,
  },
  ricochet: {
    name: "Ricochet shells",
    label: "RICOCHET",
    unit: "RICOCHET SHELLS",
    perCrate: 24,
    carryLimit: 48,
    interval: 0.85,
    damage: 80,
    speed: BASE_STANDARD_SHELL_SPEED * BASE_SPEED_MULTIPLIER,
    bounces: 3,
    color: 0xb19afc,
  },
  piercing: {
    name: "Piercing shells",
    label: "PIERCING",
    unit: "PIERCING SHELLS",
    perCrate: 24,
    carryLimit: 48,
    interval: 0.85,
    damage: 40,
    speed: BASE_STANDARD_SHELL_SPEED * BASE_SPEED_MULTIPLIER,
    bounces: 0,
    color: 0x54e6dc,
  },
};
export const LASER_DEFENSE = {
  chance: 0.5,
  duration: 6,
  range: 7,
  threatRadius: 3,
  initialDelay: 25,
  respawn: 45,
} as const;
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
