// One room's public listing as a game server announces it, and its checks, so one bad
// announcement cannot spoil the list. The limits mirror the engine's room list
// (`crates/core/src/net/room_list.rs`); `tests/ts-copies.test.ts` pins them to it.

export const MAX_LISTED_ROOMS = 256;
export const ROOM_CODE = /^[A-Z2-9]{8}$/;
/** Human seats in a room. */
export const ROOM_SEATS = 8;
export const MAX_ROUND_MINUTES = 99;
export const MAX_CONTENT_VERSION_LENGTH = 128;
export const MAP_IDS = ["village", "harbor", "quarry", "stress-test", "superstress"] as const;
export const DIFFICULTIES = ["easy", "normal", "hard"] as const;
export const PHASES = ["lobby", "playing", "results"] as const;

/** Public metadata for one room, as a server lists it. Never names, player ids or tokens. */
export interface RoomListing {
  room: string;
  contentVersion: string;
  mapMode: (typeof MAP_IDS)[number];
  difficulty: (typeof DIFFICULTIES)[number];
  humansOnly: boolean;
  roundMinutes: number;
  players: number;
  reserved: number;
  phase: (typeof PHASES)[number];
  roundId: number;
  time: number;
  scores: number[];
}

function fail(field: string): never {
  throw new Error(`Invalid room listing: ${field}`);
}
function wholeNumber(value: unknown, min: number, max: number, field: string): number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= min && value <= max
    ? value
    : fail(field);
}
function oneOf<T>(values: readonly T[], value: unknown, field: string): T {
  return values.find((item) => item === value) ?? fail(field);
}

/** One listing, copying only the declared fields. Throws on anything malformed. */
function readRoomListing(value: unknown): RoomListing {
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
    contentVersion.length > MAX_CONTENT_VERSION_LENGTH
  ) {
    fail("contentVersion");
  }
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
    roundMinutes: wholeNumber(entry.roundMinutes, 1, MAX_ROUND_MINUTES, "roundMinutes"),
    players: wholeNumber(entry.players, 0, ROOM_SEATS, "players"),
    reserved: wholeNumber(entry.reserved, 0, ROOM_SEATS, "reserved"),
    phase: oneOf(PHASES, entry.phase, "phase"),
    roundId: wholeNumber(entry.roundId, 0, Number.MAX_SAFE_INTEGER, "roundId"),
    time,
    scores: scores.map((score) => wholeNumber(score, 0, Number.MAX_SAFE_INTEGER, "scores")),
  };
}

/** A server's `rooms`: at most MAX_LISTED_ROOMS listings. Throws on anything malformed. */
export function readRooms(rooms: unknown): RoomListing[] {
  if (!Array.isArray(rooms) || rooms.length > MAX_LISTED_ROOMS) {
    fail("rooms");
  }
  return rooms.map(readRoomListing);
}
