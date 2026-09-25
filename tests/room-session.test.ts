import { afterEach, beforeEach, mock, test } from "node:test";
import assert from "node:assert/strict";
import { CONTENT_VERSION, PROTOCOL_VERSION } from "../src/net/protocol";
import type { RoomListing } from "../src/net/room-list";
import {
  DIRECTORY_HEARTBEAT_MS,
  JOIN_TIMEOUT_MS,
  MAX_PENDING_CONNECTIONS,
  MAX_SOCKET_MESSAGES_PER_SECOND,
  RoomSession,
  type RoomSocket,
} from "../server/room-session";

class FakeSocket implements RoomSocket {
  sent: { type: string; reason?: string }[] = [];
  closed?: { code: number; reason: string };
  send(text: string): void {
    this.sent.push(JSON.parse(text) as { type: string });
  }
  close(code: number, reason: string): void {
    this.closed ??= { code, reason };
  }
}
const join = (name: string) =>
  JSON.stringify({
    type: "join",
    version: PROTOCOL_VERSION,
    contentVersion: CONTENT_VERSION,
    name,
    kind: "balanced",
  });

beforeEach(() => mock.timers.enable({ apis: ["setTimeout", "Date"], now: 1_000_000 }));
afterEach(() => mock.timers.reset());

function session() {
  const listings: RoomListing[] = [];
  let ended = 0;
  const room = new RoomSession<FakeSocket>("ABCDEFGH", {
    listing: (entry) => listings.push(entry),
    ended: () => ended++,
  });
  return { room, listings, ended: () => ended };
}

test("room session refuses sockets past the pending-connection cap", () => {
  const { room } = session();
  for (let index = 0; index < MAX_PENDING_CONNECTIONS; index++)
    assert.equal(room.accept(new FakeSocket()), true);
  assert.equal(room.full, true);
  assert.equal(room.accept(new FakeSocket()), false);
  room.reset("test");
});

test("room session closes sockets that send binary, oversized or too many messages", () => {
  const { room } = session();
  const binary = new FakeSocket(),
    oversized = new FakeSocket(),
    flooding = new FakeSocket();
  for (const socket of [binary, oversized, flooding]) room.accept(socket);
  room.message(binary, new ArrayBuffer(4));
  room.message(oversized, "x".repeat(5000));
  room.message(flooding, join("flood"));
  for (let index = 0; index < MAX_SOCKET_MESSAGES_PER_SECOND; index++)
    room.message(flooding, JSON.stringify({ type: "ping", t: index, observedTick: 0 }));
  for (const socket of [binary, oversized, flooding]) assert.equal(socket.closed?.code, 1008);
  assert.equal(room.connections, 0);
  room.reset("test");
});

test("room session times out sockets that never join but keeps joined ones", () => {
  const { room } = session();
  const idle = new FakeSocket(),
    player = new FakeSocket();
  room.accept(idle);
  room.accept(player);
  room.message(player, join("player"));
  assert.equal(player.sent[0].type, "welcome");
  mock.timers.tick(JOIN_TIMEOUT_MS + 50);
  assert.deepEqual(idle.closed, { code: 1008, reason: "Join timed out" });
  assert.equal(player.closed, undefined);
  assert.equal(room.connections, 1);
  room.reset("test");
});

test("room session lists lobby changes at once and otherwise on a heartbeat", () => {
  const { room, listings } = session();
  const player = new FakeSocket();
  room.accept(player);
  room.message(player, join("player"));
  const joined = listings.length;
  assert.ok(joined > 0);
  assert.equal(listings.at(-1)!.players, 1);
  assert.equal(listings.at(-1)!.room, "ABCDEFGH");
  mock.timers.tick(DIRECTORY_HEARTBEAT_MS / 2);
  assert.equal(listings.length, joined);
  mock.timers.tick(DIRECTORY_HEARTBEAT_MS / 2 + 100);
  assert.equal(listings.length, joined + 1);
  room.reset("test");
});

test("room session reset tells players why and releases every socket", () => {
  const { room, listings, ended } = session();
  const player = new FakeSocket(),
    pending = new FakeSocket();
  room.accept(player);
  room.accept(pending);
  room.message(player, join("player"));
  room.reset("server-restart");
  assert.equal(player.sent.at(-1)!.type, "room-reset");
  assert.equal(player.sent.at(-1)!.reason, "server-restart");
  assert.equal(player.closed?.code, 1012);
  assert.equal(pending.closed?.code, 1012);
  assert.equal(listings.at(-1)!.players, 0);
  assert.equal(room.connections, 0);
  assert.equal(ended(), 1);
});

test("room session ends after the last player leaves and can host a fresh match", () => {
  const { room, ended } = session();
  const player = new FakeSocket();
  room.accept(player);
  room.message(player, join("player"));
  room.message(player, JSON.stringify({ type: "leave", roundId: 0 }));
  assert.equal(player.closed?.code, 1000);
  mock.timers.tick(100);
  assert.equal(ended(), 1);
  const next = new FakeSocket();
  assert.equal(room.accept(next), true);
  room.message(next, join("next"));
  assert.equal(next.sent[0].type, "welcome");
  room.reset("test");
});
