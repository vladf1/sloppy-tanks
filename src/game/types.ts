import type RAPIER from "@dimforge/rapier3d-compat";
import type { BotPersonality } from "./bot-personalities";
export type Team = 0 | 1;
export type VehicleKind = "scout" | "balanced" | "heavy";
export type Weapon = "standard" | "spread" | "rocket" | "ricochet" | "piercing";
export type SpecialAmmo = Exclude<Weapon, "standard">;
export type AmmoInventory = Record<SpecialAmmo, number>;
export type AmmoSelection = Weapon | -1 | 1;
export type PickupKind = SpecialAmmo | "rapid" | "shield" | "speed" | "repair" | "laser";
export interface Vec2 {
  x: number;
  z: number;
}
/** The sole input boundary for humans, bots, recordings, and future remote peers. */
export interface VehicleCommand {
  moveX: number;
  moveZ: number;
  aim: number;
  fire: boolean;
  mine: boolean;
  ammoSelection?: AmmoSelection;
}
export const idleCommand = (): VehicleCommand => ({
  moveX: 0,
  moveZ: 0,
  aim: 0,
  fire: false,
  mine: false,
});
/** Persistent entity identity. Timers are seconds; aim/heading are radians around Y. */
export interface Tank {
  id: number;
  name: string;
  team: Team;
  human: boolean;
  kind: VehicleKind;
  body: RAPIER.RigidBody;
  collider: RAPIER.Collider;
  hp: number;
  alive: boolean;
  respawn: number;
  protection: number;
  selectedAmmo: Weapon;
  ammo: AmmoInventory;
  shield: number;
  shieldPoints: number;
  rapid: number;
  speed: number;
  laser: number;
  cooldown: number;
  mineCooldown: number;
  aim: number;
  heading: number;
  previous: Vec2;
  recoil: number;
  kills: number;
  deaths: number;
  xp: number;
  lastCombat: number;
  command: VehicleCommand;
  brain: Brain;
}
/** Bot memory survives between decisions; steering and recovery update every fixed tick. */
export interface Brain {
  personality: BotPersonality;
  ultraAggressive: boolean;
  lastSeen: Vec2;
  decision: number;
  target: number;
  memory: number;
  reaction: number;
  fireDelay: number;
  aimError: number;
  path: Vec2[];
  goal: Vec2;
  last: Vec2;
  stuck: number;
  recovery: number;
  recoveryGoal: Vec2;
  recoveries: number;
  avoidance: Vec2;
  avoidanceTime: number;
  pickupTarget: number;
  navVersion: number;
  mode: "advance" | "fight" | "retreat" | "pickup" | "escort";
}
export type CoverKind =
  | "house"
  | "tree"
  | "fence"
  | "timber"
  | "concrete"
  | "wall"
  | "shed"
  | "drum"
  | "tower"
  | "rubble"
  | "boundary";
/** Axis-aligned cover footprint: w/d/h are full dimensions in world metres. */
export interface Cover extends Vec2 {
  id: number;
  kind: CoverKind;
  w: number;
  d: number;
  h: number;
  hp: number;
  maxHp: number;
  alive: boolean;
  destructible: boolean;
  body: RAPIER.RigidBody;
  collider: RAPIER.Collider;
  color: number;
  /** Chosen at collapse so rubble stays stable when its model is rebuilt. */
  debrisSeed?: number;
}
/** Planar projectile state. vx/vz are metres per second; life is remaining seconds. */
export interface Shot extends Vec2 {
  y?: number; // Render height at the muzzle; combat remains on the arena plane.
  id: number;
  owner: number;
  /** Owner's death count when fired, to keep XP attached to that life. */
  ownerLife?: number;
  team: Team;
  vx: number;
  vz: number;
  damage: number;
  bounces: number;
  life: number;
  weapon: Weapon;
  /** One shell interception for a fresh piercing round, zero otherwise. */
  piercing: number;
  /** Two fresh piercing rounds pass through each other exactly once. */
  piercedShot?: number;
  /** A defense gets one chance per projectile, even after a miss or bounce. */
  laserCheckedBy?: number[];
}
export interface Mine extends Vec2 {
  id: number;
  owner: number;
  ownerLife?: number;
  damage?: number;
  team: Team;
  arm: number;
  life: number;
}
export interface Pickup extends Vec2 {
  id: number;
  kind: PickupKind;
  available: boolean;
  cooldown: number;
}
export type WreckPart = "hull" | "turret" | "turret-barrel" | "barrel";
export interface Fragment {
  id: number;
  body: RAPIER.RigidBody;
  life: number;
  size: number;
  color: number;
  shape?: "armor" | "wheel" | "track" | "shard" | "wood";
  wreck?: VehicleKind;
  part?: WreckPart;
  cleanup?: "shrink" | "fade";
  team?: Team;
}
export type SimEvent = {
  coverKind?: CoverKind;
  height?: number;
  type:
    | "shot"
    | "impact"
    | "explosion"
    | "destroy"
    | "death"
    | "pickup"
    | "respawn"
    | "hurt"
    | "ricochet"
    | "laser"
    | "promotion";
  x: number;
  z: number;
  id?: number;
  owner?: number;
  weapon?: Weapon;
  team?: Team;
  size?: number;
  label?: string;
  color?: number;
  from?: Vec2 & { y: number };
};
export interface Match {
  phase: "ready" | "playing" | "paused" | "results";
  time: number;
  scores: [number, number];
  overtime: boolean;
  winner: Team | null;
  round: number;
}
