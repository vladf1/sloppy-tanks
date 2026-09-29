import type { ControlInput } from "./player-controls";
import type { FullState, Identity, Snapshot } from "./replication";
import { array, boolean, id, object, optional, string, enumeration, number } from "./schema";
import { team, playerKind, mapMode, difficulty } from "./scene-codec";
import {
  DEFAULT_ROUND_MINUTES,
  MAX_ROUND_MINUTES,
  type Lobby,
  type Player,
  type RoomSettings,
} from "./room-protocol";

export { DEFAULT_ROUND_MINUTES, MAX_ROUND_MINUTES, ROOM_CODE } from "./room-protocol";
export type { Lobby, Player, RoomSettings } from "./room-protocol";

declare const __MULTIPLAYER_CONTENT_VERSION__: string;
export const PROTOCOL_VERSION = 1;
export const CONTENT_VERSION =
  typeof __MULTIPLAYER_CONTENT_VERSION__ === "undefined"
    ? "test-content"
    : __MULTIPLAYER_CONTENT_VERSION__;
export const MAX_CLIENT_MESSAGE_BYTES = 4096;
export const MAX_SERVER_MESSAGE_BYTES = 1_000_000;
export const EMPTY_GRACE_MS = 30_000;
export const ROOM_IDLE_MS = 5 * 60_000;
export const roundMinutesReader = number(1, MAX_ROUND_MINUTES, true);
/** A room hosts no new battle after this long; one already under way may finish. */
export const MAX_ROOM_MS = 4 * 60 * 60_000;
/** How far a battle may run past MAX_ROOM_MS: its longest length plus overtime, so an
 * endless next-kill overtime still cannot hold a room open forever. */
export const MAX_BATTLE_OVERRUN_MS = (MAX_ROUND_MINUTES + 30) * 60_000;
export const settingsReader = object<RoomSettings>({
  mapMode,
  difficulty,
  humansOnly: boolean,
  roundMinutes: {
    read: (value: unknown) =>
      roundMinutesReader.read(value === undefined ? DEFAULT_ROUND_MINUTES : value),
  },
});
export interface Control extends Identity {
  type: "control";
  tankId: number;
  life: number;
  controlEpoch: number;
  driver: "human" | "bot" | "idle";
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
  /** ack is the latest input sequence the server applied for this seat. */
  | { type: "snapshot"; roundId: number; ack: number; snapshots: Snapshot[] }
  | { type: "pong"; t: number; tick: number }
  | { type: "error"; code: string; message: string; fatal?: boolean }
  | { type: "room-reset"; roomEpoch: string; reason: string };
/** Client messages name only the round; the socket already belongs to one room instance. */
export type InputMessage = ControlInput & { type: "input"; roundId: number };
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
