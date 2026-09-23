import { test } from "node:test";
import assert from "node:assert/strict";
import { RoomCatalog } from "../server/room-catalog";
import {
  MAX_LISTED_ROOMS,
  ROOM_LIST_TTL_MS,
  roomListReader,
  type RoomListing,
} from "../src/net/room-list";
const entry: RoomListing = {
  room: "ABCDEFGH",
  contentVersion: "test",
  mapMode: "harbor",
  difficulty: "normal",
  humansOnly: true,
  players: 1,
  reserved: 1,
  phase: "playing",
  roundId: 1,
  time: 280,
  scores: [0, 0],
};
test("room listings expire, disappear with the last connected player, and survive directory restart", () => {
  const catalog = new RoomCatalog();
  catalog.update(entry, 0);
  catalog.update({ ...entry, room: "BCDEFGHJ", players: 2, reserved: 2 }, 1);
  const restored = new RoomCatalog(catalog.saved(2));
  assert.deepEqual(
    restored.list(2).map((room) => room.players),
    [2, 1],
  );
  restored.update({ ...entry, players: 0 }, 3);
  assert.deepEqual(
    restored.list(3).map((room) => room.room),
    ["BCDEFGHJ"],
  );
  assert.equal(restored.list(ROOM_LIST_TTL_MS).length, 1);
  assert.equal(restored.list(ROOM_LIST_TTL_MS + 1).length, 0);
});
test("directory capacity is bounded and evicts the least recently refreshed listing", () => {
  const catalog = new RoomCatalog();
  for (let i = 0; i < MAX_LISTED_ROOMS + 1; i++) catalog.update({ ...entry, room: String(i) }, i);
  assert.equal(catalog.list(MAX_LISTED_ROOMS).length, MAX_LISTED_ROOMS);
  assert.ok(!catalog.list(MAX_LISTED_ROOMS).some((room) => room.room === "0"));
  catalog.update({ ...entry, room: "1", players: 4 }, MAX_LISTED_ROOMS + 1);
  assert.equal(catalog.list(MAX_LISTED_ROOMS + 1)[0].players, 4);
});
test("room list decoder rejects malformed identifiers and unbounded player counts", () => {
  assert.equal(roomListReader.read({ rooms: [entry] }).rooms.length, 1);
  assert.throws(() => roomListReader.read({ rooms: [{ ...entry, room: '"AAAAAAA' }] }));
  assert.throws(() => roomListReader.read({ rooms: [{ ...entry, players: 9 }] }));
});
