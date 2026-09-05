import type { PickupKind, VehicleKind, Weapon } from "./types";
export const STEP = 1 / 60,
  ARENA = 60,
  ROUND_TIME = 300,
  SCORE_LIMIT = 50;
// Vehicle tuning uses familiar km/h; simulation distances use world metres.
const roadSpeed = (speedKmh: number) => ({ speedKmh, speed: speedKmh / 3.6 });
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
    description: string;
  }
> = {
  scout: {
    name: "SKIPPER",
    tag: "Light scout",
    health: 80,
    ...roadSpeed(35),
    mass: 1,
    scale: 0.82,
    description: "Find the gap. Beat the turret. Never sit still.",
  },
  balanced: {
    name: "BRUISER",
    tag: "Balanced tank",
    health: 100,
    ...roadSpeed(28),
    mass: 1.45,
    scale: 1,
    description: "A little speed, a little steel. A whole lot of trouble.",
  },
  heavy: {
    name: "BIG RIG",
    tag: "Heavy tank",
    health: 140,
    ...roadSpeed(23),
    mass: 2.5,
    scale: 1.15,
    description: "Hold your ground. Punch through the mess.",
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
    speed: 19.2,
    bounces: 1,
    color: 0xffdf00,
  },
  rapid: {
    name: "Rapid fire",
    interval: 0.26,
    damage: 22,
    speed: 21.6,
    bounces: 1,
    color: 0xffa800,
  },
  spread: {
    name: "Spread shot",
    interval: 1.1,
    damage: 27,
    speed: 17.6,
    bounces: 1,
    color: 0xff38d4,
  },
  rocket: {
    name: "Breaching rockets",
    interval: 1.3,
    damage: 65,
    speed: 13.6,
    bounces: 0,
    color: 0xff591c,
  },
  ricochet: {
    name: "Ricochet rounds",
    interval: 0.7,
    damage: 40,
    speed: 22.4,
    bounces: 4,
    color: 0xb655ff,
  },
};
export const PICKUPS: Record<
  PickupKind,
  { name: string; icon: string; color: number; duration: number }
> = {
  rapid: { name: "RAPID FIRE", icon: "»", color: 0xffcf54, duration: 14 },
  spread: { name: "SPREAD SHOT", icon: "⋔", color: 0xf191cb, duration: 14 },
  rocket: { name: "BREACHER", icon: "↑", color: 0xff8a4c, duration: 14 },
  ricochet: { name: "RICOCHET", icon: "↗", color: 0xb19afc, duration: 14 },
  shield: { name: "SHIELD", icon: "◇", color: 0x72dbef, duration: 9 },
  speed: { name: "SPEED BOOST", icon: "ϟ", color: 0xbbe574, duration: 10 },
  repair: { name: "REPAIR", icon: "+", color: 0x88ddb0, duration: 0 },
};
export const TEAM_COLORS = [0x008cff, 0xff303e];
export const TEAM_NAMES = ["BLUE", "RED"];
export const GROUP = {
  tank: 0x00010007,
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
