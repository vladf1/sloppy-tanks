import assert from "node:assert/strict";
import { test } from "node:test";
import { busiestOpenRoom, type RoomListing } from "../src/net/room-list";
import { steppedMinutes } from "../src/net/room-protocol";

const room = (code: string, phase: RoomListing["phase"], players: number): RoomListing => ({
  room: code,
  contentVersion: "current",
  mapMode: "village",
  difficulty: "normal",
  humansOnly: false,
  roundMinutes: 20,
  players,
  reserved: players,
  phase,
  roundId: 1,
  time: 600,
  scores: [0, 0],
});

test("a phone joins a battle under way before a lobby, and the fuller room first", () => {
  const rooms = [
    room("LOBBY222", "lobby", 6),
    room("RESULTS2", "results", 7),
    room("PLAYING2", "playing", 2),
    room("PLAYING3", "playing", 3),
  ];
  assert.equal(busiestOpenRoom(rooms, () => true)?.room, "PLAYING3");
  assert.equal(busiestOpenRoom(rooms, (listing) => listing.phase !== "playing")?.room, "LOBBY222");
});

test("with no open room a phone creates one", () => {
  assert.equal(
    busiestOpenRoom([room("FULLROOM", "playing", 8)], () => false),
    undefined,
  );
  assert.equal(
    busiestOpenRoom([], () => true),
    undefined,
  );
});

test("the match length stepper moves by five minutes within 1 to 99", () => {
  assert.equal(steppedMinutes(20, 5), 25);
  assert.equal(steppedMinutes(20, -5), 15);
  assert.equal(steppedMinutes(22, 5), 25);
  assert.equal(steppedMinutes(22, -5), 20);
  assert.equal(steppedMinutes(5, -5), 1);
  assert.equal(steppedMinutes(1, 5), 5);
  assert.equal(steppedMinutes(97, 5), 99);
  assert.equal(steppedMinutes(Number.NaN, 5), 25);
});
