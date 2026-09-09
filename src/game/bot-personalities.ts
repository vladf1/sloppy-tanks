import { AMMO_ORDER, equippedWeapon, hasAmmo } from "./ammunition";
import { Random, WEAPONS } from "./data";
import type { Tank, VehicleKind, Weapon } from "./types";
import { rankStats } from "./veterancy";

export const BOT_PERSONALITIES = [
  "scout",
  "guard",
  "sniper",
  "heavy",
  "minelayer",
  "support",
  "artillery",
] as const;
export type BotPersonality = (typeof BOT_PERSONALITIES)[number];
interface BotProfile {
  label: string;
  chassis: VehicleKind;
  range: number;
  sight: number;
  speed: number;
  turn: number;
  reload: number;
  aimError: number;
  stationary: boolean;
}
// Adapted from v-tanks/src/game/enemy-behavior.ts. Range is scaled to our
// village lanes; snipers/artillery relocate until they have a firing lane.
export const BOT_PROFILES: Record<BotPersonality, BotProfile> = {
  scout: {
    label: "SCOUT",
    chassis: "scout",
    range: 11,
    sight: 30,
    speed: 1,
    turn: 3.3,
    reload: 1.45,
    aimError: 0.27,
    stationary: false,
  },
  guard: {
    label: "GUARD",
    chassis: "balanced",
    range: 18,
    sight: 30,
    speed: 0.78,
    turn: 3.3,
    reload: 1.15,
    aimError: 0.21,
    stationary: false,
  },
  sniper: {
    label: "SNIPER",
    chassis: "balanced",
    range: 24,
    sight: 44,
    speed: 0.7,
    turn: 1.7,
    reload: 2.3,
    aimError: 0.15,
    stationary: true,
  },
  heavy: {
    label: "HEAVY",
    chassis: "heavy",
    range: 18,
    sight: 30,
    speed: 0.7,
    turn: 3.3,
    reload: 2.05,
    aimError: 0.21,
    stationary: false,
  },
  minelayer: {
    label: "MINELAYER",
    chassis: "scout",
    range: 8,
    sight: 30,
    speed: 0.85,
    turn: 3.3,
    reload: 1.75,
    aimError: 0.25,
    stationary: false,
  },
  support: {
    label: "SUPPORT",
    chassis: "balanced",
    range: 22,
    sight: 34,
    speed: 0.78,
    turn: 3.3,
    reload: 1.15,
    aimError: 0.21,
    stationary: false,
  },
  artillery: {
    label: "ARTILLERY",
    chassis: "heavy",
    range: 26,
    sight: 42,
    speed: 0.65,
    turn: 1.7,
    reload: 3.4,
    aimError: 0.23,
    stationary: true,
  },
};

export function botAssignment(slot: number, team: number, ordinal: number) {
  // Matching frontline roles on each team, with sniper/artillery alternating
  // across the two backline slots. Count bots, not IDs shared with scenery.
  const roster: BotPersonality[] = [
    "scout",
    "guard",
    team === 0 ? "sniper" : "artillery",
    "heavy",
    "minelayer",
    "support",
    team === 0 ? "artillery" : "sniper",
  ];
  return { personality: roster[slot % roster.length], ultraAggressive: (ordinal + 1) % 10 === 0 };
}

export function botProfile(tank: Tank): BotProfile {
  return BOT_PROFILES[tank.brain.personality];
}

export const BOT_AMMO: Record<BotPersonality, Weapon> = {
  artillery: "rocket",
  heavy: "rocket",
  scout: "spread",
  minelayer: "spread",
  sniper: "piercing",
  guard: "ricochet",
  support: "ricochet",
};
export function preferredAmmo(tank: Tank): Weapon {
  const preferred = BOT_AMMO[tank.brain.personality];
  return hasAmmo(tank, preferred)
    ? preferred
    : (AMMO_ORDER.find((weapon) => weapon !== "standard" && hasAmmo(tank, weapon)) ?? "standard");
}

export function botReload(tank: Tank, jitter: number, weapon = equippedWeapon(tank)): number {
  const base = botProfile(tank).reload * (tank.brain.ultraAggressive ? 0.48 : 1);
  // Hunters close faster, but never erase the human's matched-weapon advantage.
  return (
    (Math.max(base + jitter, WEAPONS[weapon].interval * 1.15) * (tank.rapid > 0 ? 0.5 : 1)) /
    rankStats(tank).fireRate
  );
}

export function combatMovement(tank: Tank, dx: number, dz: number, strafe: number) {
  const profile = botProfile(tank);
  const d = Math.hypot(dx, dz) || 1;
  const range = tank.brain.ultraAggressive ? 7 : profile.range;
  if (d < range - 2) {
    return { x: -dx / d, z: -dz / d };
  }
  if (d > range + 2.5) {
    return { x: dx / d, z: dz / d };
  }
  if (profile.stationary && !tank.brain.ultraAggressive) {
    return { x: 0, z: 0 };
  }
  return { x: (dz / d) * strafe, z: (-dx / d) * strafe };
}

const BOT_NAMES = [
  "IRON JACK",
  "SIDEWINDER",
  "NITRO",
  "TREADHEAD",
  "HOTSHOT",
  "RIVET",
  "DUST DEVIL",
  "BULLSEYE",
  "SCRAP KING",
  "VEX",
  "BLACKTOP",
  "WRECKER",
  "FLINT",
  "GRIT",
  "BOLT",
  "ROAD RAGE",
  "CRATER",
  "SMOKESCREEN",
  "LOCKJAW",
  "RUMBLE",
  "CANNONBALL",
  "COPPERHEAD",
  "RUSTY",
  "BADGER",
  "DEADBOLT",
  "HELLCAT",
  "RICOCHET",
  "ROADBLOCK",
  "BUZZSAW",
  "CROWBAR",
  "THUNDERCLAP",
  "FLATLINE",
  "SLEDGE",
  "IRONCLAD",
  "REDLINE",
  "DIESEL",
  "DREADNOUGHT",
  "JUNKYARD",
  "SCORCH",
  "BRASS KNUCKLE",
  "WILDCARD",
  "HARDCASE",
  "RATTLER",
  "GHOST",
  "TOMBSTONE",
  "STEELTOE",
  "DUSTUP",
  "BOOMBOX",
  "HAILSTORM",
  "RAMPAGE",
  "SMOKESTACK",
  "AFTERSHOCK",
  "BACKFIRE",
  "BONEHEAD",
  "JACKHAMMER",
  "TORQUE",
  "WARBIRD",
  "BULLDOZER",
  "DYNAMO",
  "OUTLAW",
  "ROCKET DOG",
  "SIDESWIPE",
  "SPARKPLUG",
  "BARRAGE",
  "TANKBUSTER",
  "METALHEAD",
  "TRIGGER",
  "BLACKOUT",
  "WARPATH",
  "IRON WOLF",
  "SCATTERSHOT",
  "BOILER",
  "FUSE",
  "CRUNCH",
  "NIGHTSHIFT",
  "RUBBLE",
  "HEATWAVE",
  "HATCHET",
  "SHRAPNEL",
  "OVERDRIVE",
  "BULLWHIP",
  "SANDSTORM",
  "GUNSLINGER",
  "RIPSAW",
];

/** A fresh round deck; independent of combat RNG and unique until the pool is exhausted. */
export function shuffledBotNames(seed: number) {
  const names = [...BOT_NAMES];
  const rng = new Random(seed);
  for (let i = names.length - 1; i > 0; i--) {
    const j = Math.floor(rng.next() * (i + 1));
    [names[i], names[j]] = [names[j], names[i]];
  }
  return names;
}
