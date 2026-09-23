import type { PlayerAssignment } from "../game/types";
import type { ControlInput } from "./player-controls";
import type { FullState, Identity, Snapshot } from "./replication";
import { array, boolean, id, object, optional, string, enumeration, number } from "./schema";
import { team, playerKind, mapMode, difficulty } from "./scene-codec";

declare const __MULTIPLAYER_CONTENT_VERSION__: string;
export const PROTOCOL_VERSION = 1;
export const CONTENT_VERSION =
  typeof __MULTIPLAYER_CONTENT_VERSION__ === "undefined"
    ? "test-content"
    : __MULTIPLAYER_CONTENT_VERSION__;
export const MAX_CLIENT_MESSAGE_BYTES = 4096;
export const MAX_SERVER_MESSAGE_BYTES = 1_000_000;
export const ROOM_CODE = /^[A-Z2-9]{8}$/;
export const EMPTY_GRACE_MS = 30_000;
export const MAX_ROOM_MS = 30 * 60_000;
export const ROOM_IDLE_MS = 5 * 60_000;
export const DEFAULT_ROUND_MINUTES = 10;
export const roundMinutesReader = number(1, 20, true);
export const settingsReader = object({
  mapMode,
  difficulty,
  humansOnly: boolean,
  roundMinutes: {
    read: (value: unknown) =>
      roundMinutesReader.read(value === undefined ? DEFAULT_ROUND_MINUTES : value),
  },
});
export type RoomSettings = ReturnType<typeof settingsReader.read>;
export interface Player extends PlayerAssignment {
  connected: boolean;
  tankId?: number;
  kills: number;
  deaths: number;
}
export interface Lobby extends Identity {
  type: "lobby";
  phase: "lobby" | "playing" | "results";
  hostId: string;
  players: Player[];
  scoreboard: Player[];
  settings: RoomSettings;
}
export interface Control extends Identity {
  type: "control";
  tankId: number;
  life: number;
  controlEpoch: number;
  driver: "human" | "bot" | "idle";
}
export interface Ack {
  controlEpoch: number;
  inputSeq: number;
  appliedTick: number;
}
export type ServerMessage =
  | {
      type: "welcome";
      version: number;
      contentVersion: string;
      roomEpoch: string;
      playerId: string;
      token: string;
      hostId: string;
      reset?: boolean;
    }
  | Lobby
  | Control
  | FullState
  | { type: "snapshot"; ack: Ack; snapshots: Snapshot[] }
  | { type: "pong"; t: number; tick: number }
  | { type: "error"; code: string; message: string; fatal?: boolean }
  | { type: "room-reset"; roomEpoch: string; reason: string };
export type InputMessage = ControlInput & Identity & { type: "input" };
export const joinReader = object({
  version: id,
  contentVersion: string(128, 1),
  name: string(24, 1),
  kind: playerKind,
  team: optional(team),
  token: optional(string(128, 16)),
  roomEpoch: optional(string(128, 1)),
  create: optional(settingsReader),
  existingRoom: optional(boolean),
});
export const playerReader = object<Player>({
  playerId: string(128, 1),
  name: string(24, 1),
  team,
  slot: id,
  kind: playerKind,
  connected: boolean,
  tankId: optional(id),
  kills: id,
  deaths: id,
});
export const lobbyReader = object<Lobby>({
  type: enumeration("lobby"),
  roomEpoch: string(128, 1),
  roundId: id,
  phase: enumeration("lobby", "playing", "results"),
  hostId: string(128),
  players: array(playerReader, 8),
  scoreboard: array(playerReader, 128),
  settings: settingsReader,
});
export const controlReader = object<Control>({
  type: enumeration("control"),
  roomEpoch: string(128, 1),
  roundId: id,
  tankId: id,
  life: id,
  controlEpoch: id,
  driver: enumeration("human", "bot", "idle"),
});
export const ackReader = object<Ack>({ controlEpoch: id, inputSeq: id, appliedTick: id });
