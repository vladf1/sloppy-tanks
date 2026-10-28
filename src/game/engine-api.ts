/** The page's contract with the Rust engine (`crates/web/src/game.rs`): packed input
 * and frame-result slots, and the shapes of its JSON reports. This module imports
 * only types, so shell modules can use it without loading the engine. */
import type { MapId } from "./map-options";
import type { EndCause, Lobby } from "../net/room-protocol";

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
  stickTurn: 12,
  lookPixels: 13,
  zoom: 14,
  toggleView: 15,
  wheelAmmo: 16,
  length: 17,
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
  /** The viewer's tank on screen (NDC, y up); NaN in first person or off screen. */
  viewerX: 13,
  viewerY: 14,
  /** 0 when the engine skipped the frame while the GPU still ran earlier ones
   * (`framesInFlight`); the other slots then repeat the last frame's. */
  ran: 15,
} as const;

/** The engine config's `framesInFlight` for this browser: how many frames may still be
 * on the GPU when the next one draws. Chrome holds animation frames back while the GPU
 * is behind, so 0 leaves the pacing to it (frame callbacks it reports late would only
 * skip frames). WebKit (Safari, and every iOS browser) lets a WebGPU canvas's frames
 * queue instead: a GPU-bound village kept 0.4-0.9 s of frames waiting and the controls
 * answered that late, so there two keep the GPU fed while the picture stays current. */
export function framesInFlight(userAgent: string): number {
  return /AppleWebKit/.test(userAgent) && !/Chrome|Chromium|Android/.test(userAgent) ? 2 : 0;
}

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
  | "promotion"
  | "shatter";

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

/** `NetGame.take_notices`' connection progress, with the connection dialog's words while
 * the socket is down. The room menu's words come from `crates/web/src/room_text.rs`. */
export type RoomStatus =
  | { text: string; connected: true }
  | { text: string; connected: false; title: string; message: string };

/** Why a room stopped for good, with the dialog's title and buttons. `retry` joins again
 * and is null where that cannot work; `setup` leaves for Battle Setup. */
export interface RoomEnding {
  cause: EndCause;
  text: string;
  title: string;
  retry: string | null;
  setup: string;
}

/** The room menu's words for a lobby. */
export interface RoomMenuText {
  title: string;
  hint: string;
  /** The rules as chips: the arena, the bots and the match length. */
  summary: string[];
}

/** A player who joined the room or came back, as a kill feed line. */
export interface RoomArrival {
  text: string;
  team: Team;
}

/** A new lobby: the wire record, the viewer's id in it, the menu's words for it and who
 * joined or came back since the last one. */
export interface RoomLobby {
  lobby: Lobby;
  playerId: string;
  menu: RoomMenuText;
  arrivals: RoomArrival[];
}

/** What wrecked the viewer's tank, for the respawn card (`DeathReport` in
 * `crates/web/src/feedback.rs`). */
export interface DeathReport {
  /** The killer's name; null for the yard, a departed player or the viewer's own
   * ordnance. */
  killer: string | null;
  self: boolean;
  cause: DamageCause | null;
  /** The weapon's label, such as "Rocket blast", or null when it is unknown. */
  weapon: string | null;
  /** The weapon's icon: a `HUD_ICONS` symbol after its `hud-` prefix. */
  icon: string;
  /** The killer's vehicle name, team and kills, when the scoreboard knows them. */
  killerVehicle?: string;
  killerTeam?: Team;
  killerKills?: number;
  /** All of it as a sentence, for the toast and the respawn card's status line. */
  sentence: string;
}

/** Which side a kill-feed name is drawn in: a team's colour, or neutral for the yard. */
export type FeedSide = "blue" | "red" | "neutral";

/** A kill-feed row as the engine reports it on a death. */
export interface EngineFeedRow {
  /** Killer and victim: "YOU" for the viewer, "YARD" for nobody's ordnance. */
  names: [string, string];
  sides: [FeedSide, FeedSide];
  cause?: DamageCause;
  /** The weapon's icon: a `HUD_ICONS` symbol after its `hud-` prefix. */
  icon: string;
  /** The viewer destroyed someone else (by tank identity, not by name). */
  ownKill: boolean;
  /** The viewer's own wreck. */
  ownDeath: boolean;
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
  /** The human's death: who and what wrecked it. */
  deathReport?: DeathReport;
  /** Every tank's death: its kill-feed row. */
  feed?: EngineFeedRow;
  /** The human's pickups, promotions and death: the toast over the field. */
  toast?: { text: string; tone: "pickup" | "promotion" | "death" };
}

/** A battle sound, the file `audio/<name>.mp3`. */
export type SoundName =
  | "shot"
  | "shot-spread"
  | "shot-rocket"
  | "shot-ricochet"
  | "shot-piercing"
  | "explosion"
  | "wood-break"
  | "rubble-break"
  | "impact"
  | "pickup"
  | "hit"
  | "laser"
  | "promotion";

/** A sound the engine chose for the batch's events (`crates/web/src/sounds.rs`). */
export interface PlannedSound {
  sound: SoundName;
  /** Linear volume before the master volume. */
  gain: number;
  /** Stereo position, -1 left to 1 right. */
  pan: number;
}

export interface EventBatch {
  listener: Point;
  /** World X/Z of screen right; first person turns it with the view. */
  listenerRight: Point;
  events: EngineEvent[];
  /** The events' sounds in the order to start them, already rate-limited. */
  sounds: PlannedSound[];
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
  kind: VehicleKind;
  vehicleName: string;
  alive: boolean;
  hp: number;
  maxHp: number;
  healthRatio: number;
  healthColor: number;
  rank: number;
  rankName: string;
  rankDamage: number;
  rankFireRate: number;
  rankHealth: number;
  rankRepair: number;
  /** Seconds out of combat before a veteran repairs. */
  repairDelay: number;
  ammo: AmmoSlot[];
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
  /** How the hull reads at a glance, at the overhead bar's thresholds. */
  hullTone: "healthy" | "damaged" | "critical";
  /** The tank panel's hull colour along its ramp (sRGB hex). */
  hullRamp: number;
  /** The rank's benefits, for its tooltip. */
  rankTitle: string;
  /** The timed power-ups and protections running, in the chips' order. */
  effects: ActiveEffect[];
}

/** One running power-up or protection, as the tank panel's chips show it. */
export interface ActiveEffect {
  kind: "spawn" | "shield" | "rapid" | "boost" | "laser" | "repair";
  name: string;
  /** What is left: seconds, shield points and seconds, or nothing. */
  detail: string;
  /** The share of its time left, for its timer ring; 1 for a lasting effect. */
  share: number;
}

/** The score pill between the team plates. */
export interface ScorePill {
  /** The goal under the clock; null in a room. */
  objective: string | null;
  /** "NEXT KILL" in overtime; the clock shows `seconds` otherwise. */
  clockLabel: string | null;
  seconds: number;
}

export interface ScoreboardRow {
  id: number;
  name: string;
  kills: number;
  deaths: number;
  team: Team;
  kind: VehicleKind;
  /** A player's tank rather than a bot's. */
  human: boolean;
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
  report: BattleReport;
}

/** How the round came out for the viewer: the results dialog's `data-outcome`. A battle
 * the player ended is called by where the team stood. */
export type Outcome =
  "won" | "lost" | "draw" | "ended" | "ended-ahead" | "ended-behind" | "ended-even";

/** A stat tile of the battle report. A record stars its footer. */
export interface ReportTile {
  label: string;
  hint: string;
  value: string;
  footer: string;
  record: boolean;
  standout: boolean;
}

export interface BoardTeam {
  /** "Blue team". */
  label: string;
  /** The team's tank ids, most kills first. */
  tanks: number[];
}

/** The battle report in words (`crates/web/src/battle_report.rs`); the page lays it out. */
export interface BattleReport {
  outcome: Outcome;
  title: string;
  /** A solo run's loss says "One more run?", led by the death's sentence. */
  lede: string;
  /** Kills (the hero tile) first, then the featured metrics and accuracy. */
  tiles: ReportTile[];
  /** The hero tile's line; `meter` fills toward the best, none for a record or a tie. */
  progress: { text: string; record: boolean; meter: number | null };
  details: { label: string; hint: string; value: string; record: boolean }[];
  /** `record` is starred and bold before `text`; a warning has none. */
  note: { record: string | null; text: string } | null;
  board: { teams: [BoardTeam, BoardTeam]; mvp: number | null };
}

/** `pause_json`: pause's facts, and a team battle's line under its title. */
export interface PauseReport {
  tally: { kills: number; survived: string } | null;
  standing: { text: string; tone: "ahead" | "behind" | "level" } | null;
  facts: { label: string; value: string; note: string }[];
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
  scorePill: ScorePill;
}

/** `stats_json().quality`: the graphics tier and the dynamic render scale
 * (`crates/render/src/quality.rs`). */
export interface QualityStats {
  /** The Graphics setting: "auto", "high", "medium" or "low". */
  setting: string;
  /** The tier in effect: "high", "medium" or "low". */
  tier: string;
  /** Scene pixels per canvas pixel along each axis; the output pass upsamples. */
  scale: number;
  /** Scene pixels per CSS pixel along each axis. */
  renderRatio: number;
  /** The scene's width and height in pixels. */
  scene: [number, number];
  /** The GPU's frame time in milliseconds, smoothed, where the browser reports when
   * the GPU finished a frame (WebGPU); null on WebGL. */
  gpuMs: number | null;
  /** Auto's tier or scale changes during this page session, and the latest ones. */
  changes: number;
  log: {
    at: number;
    tier: string;
    scale: number;
    reason: string;
    meanMs: number;
    gpuMs: number | null;
  }[];
}

/** A row of `nerd_stats_json()`: the engine writes the value and what it means. */
export interface StatsRow {
  label: string;
  value: string;
  hint: string;
}

/** `nerd_stats_json()`: the Stats for nerds panel's sections in order
 * (`crates/web/src/nerd_rows.rs`). */
export interface StatsSection {
  title: string;
  rows: StatsRow[];
}

/** `stats_json`: Stats for nerds' readings, for diagnostics and the frame recorder. */
export interface EngineStats {
  frameMs: number;
  fps: number;
  simMs: number;
  renderMs: number;
  /** The engine build's browser API: "WebGPU", or "WebGL" for the fallback. */
  graphicsApi: string;
  quality: QualityStats;
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
  elapsed: number;
  pixelRatio: number;
}
