/** The page's contract with the Rust engine (`crates/web/src/game.rs`): packed input
 * and frame-result slots, and the shapes of its JSON reports. This module imports
 * nothing, so shell modules can use it without loading the engine. */
import type { MapId } from "./map-options";

/** Slots of the packed raw input frame (`sloppy_render::presentation::input::slot`).
 * Booleans are 0 or 1; one-shot slots count presses since the previous frame. */
export const INPUT = {
  up: 0,
  down: 1,
  left: 2,
  right: 3,
  touchMoveX: 4,
  touchMoveZ: 5,
  fire: 6,
  mine: 7,
  ammoSlot: 8,
  ammoStep: 9,
  pointerX: 10,
  pointerY: 11,
  touchAiming: 12,
  touchAimX: 13,
  touchAimY: 14,
  aimStickHeld: 15,
  lookPixels: 16,
  zoom: 17,
  toggleView: 18,
  wheelAmmo: 19,
  length: 20,
} as const;

/** Slots of `Game.frame`'s result (`frame_slot` in `game.rs`). */
export const FRAME = {
  phase: 0,
  humanAlive: 1,
  cockpit: 2,
  hullAngle: 3,
  events: 4,
  clearInput: 5,
  hudDue: 6,
  simMs: 7,
  renderMs: 8,
  firstPerson: 9,
  dt: 10,
} as const;

export const PHASES = ["ready", "playing", "paused", "results"] as const;
export type Phase = (typeof PHASES)[number];
export type Team = 0 | 1;
export type Weapon = "standard" | "spread" | "rocket" | "ricochet" | "piercing" | "tow";
export type VehicleKind = "scout" | "balanced" | "heavy" | "humvee";
export type PlayerVehicleKind = Exclude<VehicleKind, "humvee">;
export type GameMode = "team" | "solo";
export type Difficulty = "easy" | "normal" | "hard";
export type CoverKind =
  | "rock"
  | "teeth"
  | "hedgehog"
  | "container"
  | "cargo"
  | "house"
  | "tree"
  | "timber"
  | "concrete"
  | "drum"
  | "tower"
  | "rubble"
  | "boundary";
export type DamageCause = Weapon | "mine" | "drum" | "interception" | "explosion";
export type EventType =
  | "debris-impact"
  | "notice"
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

export interface Point {
  x: number;
  z: number;
}

export interface MatchState {
  phase: Phase;
  time: number;
  scores: [number, number];
  overtime: boolean;
  endedEarly?: boolean;
  winner: Team | null;
  round: number;
}

/** A simulation event as `drain_events` reports it. */
export interface EngineEvent {
  type: EventType;
  x: number;
  z: number;
  id?: number;
  owner?: number;
  team?: Team;
  weapon?: Weapon;
  label?: string;
  coverKind?: CoverKind;
  damageSource?: { cause: DamageCause; origin: Point };
  /** The human's own hit on an enemy. */
  playerHit: boolean;
  /** The event concerns the human's tank. */
  own: boolean;
  /** Clockwise screen angle (radians) of damage the human took, or null. */
  damageAngle: number | null;
}

export interface EventBatch {
  listener: Point;
  /** World X/Z of screen right; first person turns it with the view. */
  listenerRight: Point;
  events: EngineEvent[];
}

export interface AmmoSlot {
  weapon: Weapon;
  /** null for unlimited Standard shells. */
  count: number | null;
  selected: boolean;
  available: boolean;
}

export interface HumanState {
  id: number;
  name: string;
  kind: VehicleKind;
  vehicleName: string;
  team: Team;
  alive: boolean;
  hp: number;
  maxHp: number;
  healthRatio: number;
  healthColor: number;
  xp: number;
  rank: number;
  rankName: string;
  rankDamage: number;
  rankFireRate: number;
  rankHealth: number;
  rankRepair: number;
  /** Seconds out of combat before a veteran repairs. */
  repairDelay: number;
  equipped: Weapon;
  ammo: AmmoSlot[];
  cooldown: number;
  mineCooldown: number;
  protection: number;
  shield: number;
  shieldPoints: number;
  rapid: number;
  speed: number;
  laser: number;
  respawn: number;
  kills: number;
  deaths: number;
  selfRepair: boolean;
}

export interface ScoreboardRow {
  id: number;
  name: string;
  team: Team;
  kind: VehicleKind;
  human: boolean;
  alive: boolean;
  kills: number;
  deaths: number;
  rank: number;
}

export type RecapMetric =
  | "kills"
  | "damage"
  | "bestLife"
  | "rank"
  | "busiestMinute"
  | "longestLife"
  | "multikill"
  | "clutchKills"
  | "revengeKills"
  | "posthumousKills"
  | "mineKills"
  | "coverDestroyed"
  | "pickups";

/** The finished round's report; the engine decides feats and personal bests. */
export interface RecapState {
  stats: Record<RecapMetric, number>;
  best: Record<RecapMetric, number>;
  improved: RecapMetric[];
  persisted: boolean;
  feats: { title: string; detail: string }[];
  shots: number;
  directHits: number;
  damageTaken: number;
  shieldAbsorbed: number;
  rankNames: string[];
}

/** `hud_json`: everything the HUD and menus show. */
export interface HudState {
  match: MatchState;
  elapsed: number;
  gameMode: GameMode;
  endlessMatch: boolean;
  mapMode: MapId;
  mapName: string;
  difficulty: Difficulty;
  humanTeam: Team;
  scoreLimit: number;
  teamNames: [string, string];
  activeEnemies: number;
  speedTuning: { "tank-speed": number; "bullet-speed": number };
  human: HumanState;
  scoreboard: ScoreboardRow[];
  recap: RecapState | null;
}

/** `stats_json`: Stats for nerds. */
export interface EngineStats {
  frameMs: number;
  averageFrameMs: number;
  fps: number;
  simMs: number;
  renderMs: number;
  /** The engine build's browser API: "WebGPU", or "WebGL" for the fallback. */
  graphicsApi: string;
  drawCalls: number;
  triangles: number;
  shadowDrawCalls: number;
  reflectionDrawCalls: number;
  shadowTriangles: number;
  reflectionTriangles: number;
  mainTriangles: number;
  pipelines: number;
  latePipelines: number;
  meshes: number;
  materials: number;
  textures: number;
  texturesPending: number;
  buffers: number;
  instances: number;
  gpuBytes: number;
  /** Mesh page bytes no mesh uses, included in `gpuBytes`. */
  meshSlackBytes: number;
  bodies: number;
  fixedBodies: number;
  dynamicBodies: number;
  sleepingBodies: number;
  colliders: number;
  shots: number;
  mines: number;
  fragments: number;
  maxFragments: number;
  tanks: number;
  tanksAlive: number;
  pickups: number;
  pickupsReady: number;
  particles: number;
  effectInstances: number;
  elapsed: number;
  pixelRatio: number;
}
