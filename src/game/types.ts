import type { BotPersonality } from "./bot-personalities";
import type RAPIER from "@dimforge/rapier3d-compat";
export type Team = 0 | 1;
export type VehicleKind = "scout" | "balanced" | "heavy";
export type Weapon = "standard" | "spread" | "rocket";
export type PickupKind =
  Exclude<Weapon, "standard"> | "rapid" | "ricochet" | "shield" | "speed" | "repair";
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
}
export const idleCommand = (): VehicleCommand => ({
  moveX: 0,
  moveZ: 0,
  aim: 0,
  fire: false,
  mine: false,
});
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
  spread: number;
  rocket: number;
  shield: number;
  shieldPoints: number;
  rapid: number;
  ricochet: number;
  speed: number;
  cooldown: number;
  mineCooldown: number;
  aim: number;
  heading: number;
  previous: Vec2;
  recoil: number;
  kills: number;
  deaths: number;
  command: VehicleCommand;
  brain: Brain;
}
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
  navVersion: number;
  mode: "advance" | "fight" | "retreat" | "pickup" | "escort";
}
export type CoverKind =
  | "house"
  | "tree"
  | "fence"
  | "concrete"
  | "wall"
  | "shed"
  | "drum"
  | "tower"
  | "rubble"
  | "boundary";
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
}
export interface Shot extends Vec2 {
  y?: number; // Render height at the muzzle; combat remains on the arena plane.
  id: number;
  owner: number;
  team: Team;
  vx: number;
  vz: number;
  damage: number;
  bounces: number;
  life: number;
  weapon: Weapon;
}
export interface Mine extends Vec2 {
  id: number;
  owner: number;
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
  shape?: "armor" | "wheel" | "track" | "shard";
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
    | "ricochet";
  x: number;
  z: number;
  id?: number;
  team?: Team;
  size?: number;
  label?: string;
  color?: number;
};
export interface Match {
  phase: "ready" | "playing" | "paused" | "results";
  time: number;
  scores: [number, number];
  overtime: boolean;
  winner: Team | null;
  round: number;
}
