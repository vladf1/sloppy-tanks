import type { PickupKind, VehicleKind, Weapon } from "./types";
export const STEP = 1 / 60,
  ARENA = 60,
  ROUND_TIME = 300,
  SCORE_LIMIT = 50;
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
    scale: 0.82,
  },
  balanced: {
    name: "BRUISER",
    tag: "Balanced tank",
    health: 100,
    ...referenceSpeed(1),
    mass: 1.45,
    scale: 1,
  },
  heavy: {
    name: "BIG RIG",
    tag: "Heavy tank",
    health: 140,
    ...referenceSpeed(0.76),
    mass: 2.5,
    scale: 1.15,
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
  }
> = {
  standard: {
    name: "Standard shells",
    interval: 0.85,
    damage: 40,
    speed: 19.2 * BASE_SPEED_MULTIPLIER,
    bounces: 1,
    color: 0xffdf00,
  },
  spread: {
    name: "Spread shot",
    interval: 1.1,
    damage: 27,
    speed: 17.6 * BASE_SPEED_MULTIPLIER,
    bounces: 1,
    color: 0xff38d4,
  },
  rocket: {
    name: "Breaching rockets",
    interval: 1.3,
    damage: 65,
    speed: 13.6 * BASE_SPEED_MULTIPLIER,
    bounces: 0,
    color: 0xff591c,
  },

};
export const PICKUPS: Record<
  PickupKind,
  { name: string; icon: string; color: number; duration: number }
> = {
  rapid: { name: "RAPID FIRE", icon: "»", color: 0xffcf54, duration: 12 },
  spread: { name: "SPREAD SHOT", icon: "⋔", color: 0xf191cb, duration: 14 },
  rocket: { name: "BREACHER", icon: "↑", color: 0xff8a4c, duration: 14 },
  ricochet: { name: "RICOCHET CORE", icon: "↗", color: 0xb19afc, duration: 12 },
  shield: { name: "SHIELD", icon: "◇", color: 0x72dbef, duration: 15 },
  speed: { name: "SPEED BOOST", icon: "ϟ", color: 0xbbe574, duration: 12 },
  repair: { name: "REPAIR", icon: "+", color: 0x88ddb0, duration: 0 },
};
export const TEAM_COLORS = [0x008cff, 0xff303e];
export const TEAM_NAMES = ["BLUE", "RED"];
export const GROUP = {
  tank: 0x00010006, // Compact collider touches cover and ground only.
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
