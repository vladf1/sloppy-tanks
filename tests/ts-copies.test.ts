import { test } from "node:test";
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
import {
  DIFFICULTIES,
  MAP_IDS,
  MAX_CONTENT_VERSION_LENGTH,
  MAX_LISTED_ROOMS,
  MAX_ROUND_MINUTES,
  PHASES as ROOM_PHASES,
  ROOM_CODE,
  ROOM_SEATS,
} from "../directory/room-listing";
import { FRAME, INPUT, PHASES } from "../tools/engine-tables";

// The directory and the game preview keep their own copies of these engine tables.
// `crates/web/src/ts_copies_tests.rs` writes the Rust values to the fixture; rerun it
// with SLOPPY_UPDATE_FIXTURES=1 after a deliberate change.
interface Copies {
  rooms: {
    maxListedRooms: number;
    seats: number;
    maxContentVersionLength: number;
    codeLength: number;
    codeCharacters: string;
    maxRoundMinutes: number;
    mapModes: string[];
    difficulties: string[];
    phases: string[];
  };
  inputSlots: Record<string, number>;
  frameSlots: Record<string, number>;
  phases: Record<string, number>;
}

const copies = JSON.parse(
  readFileSync(new URL("../crates/web/tests/fixtures/ts-copies.json", import.meta.url), "utf8"),
) as Copies;

test("the directory reads room listings within the engine's limits", () => {
  const { rooms } = copies;
  assert.equal(MAX_LISTED_ROOMS, rooms.maxListedRooms);
  assert.equal(ROOM_SEATS, rooms.seats);
  assert.equal(MAX_ROUND_MINUTES, rooms.maxRoundMinutes);
  assert.equal(MAX_CONTENT_VERSION_LENGTH, rooms.maxContentVersionLength);
  assert.deepEqual([...MAP_IDS], rooms.mapModes);
  assert.deepEqual([...DIFFICULTIES], rooms.difficulties);
  assert.deepEqual([...ROOM_PHASES], rooms.phases);
  const printable = Array.from({ length: 95 }, (_, index) => String.fromCharCode(32 + index));
  const codeCharacters = printable
    .filter((character) => ROOM_CODE.test(character.repeat(rooms.codeLength)))
    .join("");
  assert.equal(codeCharacters, rooms.codeCharacters);
  assert.ok(!ROOM_CODE.test("A".repeat(rooms.codeLength + 1)));
});

test("the game preview's slots and phases are the engine's", () => {
  assert.deepEqual({ ...INPUT }, copies.inputSlots);
  for (const [name, index] of Object.entries(FRAME)) {
    assert.equal(index, copies.frameSlots[name], `frame slot ${name}`);
  }
  for (const [name, code] of Object.entries(copies.phases)) {
    assert.equal(PHASES[code], name, `phase ${code}`);
  }
});
