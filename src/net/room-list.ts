import { array, boolean, enumeration, id, number, object, string } from "./schema";
import { ROOM_CODE, DEFAULT_ROUND_MINUTES, roundMinutesReader } from "./protocol";
import { difficulty, mapMode } from "./scene-codec";

export const MAX_LISTED_ROOMS = 256;
export const ROOM_LIST_TTL_MS = 45_000;
export const roomListingReader = object({
  room: {
    read(value: unknown): string {
      const code = string(8, 8).read(value);
      if (!ROOM_CODE.test(code)) {
        throw new Error("Invalid room code");
      }
      return code;
    },
  },
  contentVersion: string(128, 1),
  mapMode,
  difficulty,
  humansOnly: boolean,
  roundMinutes: {
    read: (value: unknown) =>
      roundMinutesReader.read(value === undefined ? DEFAULT_ROUND_MINUTES : value),
  },
  players: number(0, 8, true),
  reserved: number(0, 8, true),
  phase: enumeration("lobby", "playing", "results"),
  roundId: id,
  time: number(0, 3600),
  scores: array(id, 2),
});
export type RoomListing = ReturnType<typeof roomListingReader.read>;
export const roomListReader = object({ rooms: array(roomListingReader, MAX_LISTED_ROOMS) });
