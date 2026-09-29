import type { MapId } from "../game/map-options";
import type { Difficulty, PlayerVehicleKind, Team } from "../game/engine-api";

/** Room records and limits the page shows: types and constants only. The connection,
 * replication and input run in the Rust engine (`NetGame`); the wire protocol itself
 * lives in `crates/core/src/net`. */

export const ROOM_CODE = /^[A-Z2-9]{8}$/;
export const DEFAULT_ROUND_MINUTES = 20;
export const MAX_ROUND_MINUTES = 99;
export const PLAYER_KINDS = ["scout", "balanced", "heavy"] as const satisfies PlayerVehicleKind[];
export type { Difficulty };
export type RoomPhase = "lobby" | "playing" | "results";

/** The host's rules for the next battle. */
export interface RoomSettings {
  mapMode: MapId;
  difficulty: Difficulty;
  humansOnly: boolean;
  roundMinutes: number;
}

/** A seat as the lobby lists it. */
export interface Player {
  playerId: string;
  name: string;
  team: Team;
  slot: number;
  kind: PlayerVehicleKind;
  connected: boolean;
  tankId?: number;
  kills: number;
  deaths: number;
}

/** The server's `lobby` message. */
export interface Lobby {
  type: "lobby";
  roomEpoch: string;
  roundId: number;
  phase: RoomPhase;
  hostId: string;
  players: Player[];
  scoreboard: Player[];
  settings: RoomSettings;
}

/** A player's choices for joining a room. */
export interface JoinChoice {
  name: string;
  kind: PlayerVehicleKind;
  team?: Team;
  create?: RoomSettings;
  existingRoom?: boolean;
}

/** Why a connection stopped retrying. Each cause offers its own way back into a game. */
export type EndCause =
  /** The server stayed unreachable for the whole reconnect window. */
  | "lost"
  /** The room itself is gone: a server restart, a time limit or a server fault. */
  | "room-ended"
  /** The server let the seat go; joining again takes a new one. */
  | "seat-expired"
  /** The same seat connected from another tab, which now drives the tank. */
  | "other-tab"
  /** This page and the server run different game versions. */
  | "outdated"
  /** WebGPU reported a validation error or lost the device; only a reload recovers. */
  | "renderer"
  | "rejected";
export interface ConnectionEnd {
  cause: EndCause;
  text: string;
}

/** Dev-only URL parameters that add transport delay to the room connection. */
export const TRANSPORT_DELAY_PARAMS = ["latency", "jitter", "stall"] as const;

export function isPlayerKind(value: unknown): value is PlayerVehicleKind {
  return PLAYER_KINDS.some((kind) => kind === value);
}

/** Whole minutes from 1 to MAX_ROUND_MINUTES. */
export function isRoundMinutes(value: unknown): value is number {
  return (
    typeof value === "number" && Number.isInteger(value) && value >= 1 && value <= MAX_ROUND_MINUTES
  );
}
