import { MAP_IDS, type MapId } from "../game/map-options";
import {
  DEFAULT_ROUND_MINUTES,
  MAX_ROUND_MINUTES,
  ROOM_CODE,
  isRoundMinutes,
  type Difficulty,
  type RoomPhase,
} from "./room-protocol";

export const MAX_LISTED_ROOMS = 256;
export const ROOM_LIST_TTL_MS = 45_000;
const ROOM_SEATS = 8;
const DIFFICULTIES: readonly Difficulty[] = ["easy", "normal", "hard"];
const PHASES: readonly RoomPhase[] = ["lobby", "playing", "results"];

/** Public metadata for one room, as `/rooms` lists it. Never names, player ids or tokens. */
export interface RoomListing {
  room: string;
  contentVersion: string;
  mapMode: MapId;
  difficulty: Difficulty;
  humansOnly: boolean;
  roundMinutes: number;
  players: number;
  reserved: number;
  phase: RoomPhase;
  roundId: number;
  time: number;
  scores: number[];
}

function fail(field: string): never {
  throw new Error(`Invalid room listing: ${field}`);
}
function wholeNumber(value: unknown, max: number, field: string): number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 && value <= max
    ? value
    : fail(field);
}
function oneOf<T>(values: readonly T[], value: unknown, field: string): T {
  return values.find((item) => item === value) ?? fail(field);
}

/** One listing, copying only the declared fields. Throws on anything malformed. */
export function readRoomListing(value: unknown): RoomListing {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    fail("entry");
  }
  const entry = value as Record<string, unknown>;
  const room = entry.room;
  if (typeof room !== "string" || !ROOM_CODE.test(room)) {
    fail("room");
  }
  const contentVersion = entry.contentVersion;
  if (
    typeof contentVersion !== "string" ||
    contentVersion.length < 1 ||
    contentVersion.length > 128
  ) {
    fail("contentVersion");
  }
  const roundMinutes = entry.roundMinutes ?? DEFAULT_ROUND_MINUTES;
  const time = entry.time;
  if (typeof time !== "number" || !(time >= 0 && time <= MAX_ROUND_MINUTES * 60)) {
    fail("time");
  }
  const scores = entry.scores;
  if (!Array.isArray(scores) || scores.length > 2) {
    fail("scores");
  }
  if (typeof entry.humansOnly !== "boolean") {
    fail("humansOnly");
  }
  return {
    room,
    contentVersion,
    mapMode: oneOf(MAP_IDS, entry.mapMode, "mapMode"),
    difficulty: oneOf(DIFFICULTIES, entry.difficulty, "difficulty"),
    humansOnly: entry.humansOnly,
    roundMinutes: isRoundMinutes(roundMinutes) ? roundMinutes : fail("roundMinutes"),
    players: wholeNumber(entry.players, ROOM_SEATS, "players"),
    reserved: wholeNumber(entry.reserved, ROOM_SEATS, "reserved"),
    phase: oneOf(PHASES, entry.phase, "phase"),
    roundId: wholeNumber(entry.roundId, Number.MAX_SAFE_INTEGER, "roundId"),
    time,
    scores: scores.map((score) => wholeNumber(score, Number.MAX_SAFE_INTEGER, "scores")),
  };
}

/** The `/rooms` response: `{ rooms: RoomListing[] }`, at most MAX_LISTED_ROOMS. */
export function readRoomList(value: unknown): RoomListing[] {
  const rooms = (value as { rooms?: unknown } | null)?.rooms;
  if (!Array.isArray(rooms) || rooms.length > MAX_LISTED_ROOMS) {
    fail("rooms");
  }
  return rooms.map(readRoomListing);
}
