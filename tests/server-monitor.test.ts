import { test } from "node:test";
import assert from "node:assert/strict";
import { ServerMonitor } from "../server/monitor";
import type { RoomSample } from "../server/room-session";

const room: RoomSample = {
  room: "ABCDEFGH",
  mapMode: "harbor",
  phase: "playing",
  players: 3,
  seats: 4,
  sockets: 3,
  timeLeft: 125,
  scores: [4, 7],
  ageSeconds: 60,
  tick: 3600,
  debtMs: 0,
  tickAvgMs: 2.5,
  tickMaxMs: 9,
  sentBytes: 1024 * 100,
  receivedBytes: 1024,
};

test("server monitor summarizes each minute while active, then logs one idle summary", () => {
  const lines: string[] = [];
  let rooms = [room];
  const monitor = new ServerMonitor(
    { samples: () => rooms, sockets: () => rooms.length * 3 },
    (line) => lines.push(line),
  );
  for (let sample = 0; sample < 5; sample++) monitor.sample();
  assert.deepEqual(lines, []);
  const stats = monitor.sample();
  assert.equal(stats.players, 3);
  assert.equal(stats.roomList[0].room, "ABCDEFGH");
  assert.equal(lines.length, 2);
  assert.match(lines[0], /^stats: 1 rooms, 3 players, 3 sockets \| out /);
  assert.match(lines[1], /ABCDEFGH harbor playing 3\/4 players 2m 5s left 4-7/);

  rooms = [];
  for (let sample = 0; sample < 6; sample++) monitor.sample();
  assert.equal(lines.length, 3);
  assert.match(lines[2], /^stats: 0 rooms, 0 players/);
  for (let sample = 0; sample < 12; sample++) monitor.sample();
  assert.equal(lines.length, 3);
});

test("server monitor logs room lifecycle in plain lines", () => {
  const lines: string[] = [];
  const monitor = new ServerMonitor({ samples: () => [], sockets: () => 0 }, (line) =>
    lines.push(line),
  );
  monitor.activity("ABCDEFGH", { type: "created" });
  monitor.activity("ABCDEFGH", { type: "joined", players: 1 });
  monitor.activity("ABCDEFGH", { type: "left", players: 0, code: 1006 });
  monitor.activity("ABCDEFGH", { type: "closed", players: 0, code: 1000, reason: "Left room" });
  monitor.activity("ABCDEFGH", {
    type: "closed",
    players: 0,
    code: 1008,
    reason: "Join timed out",
  });
  monitor.ended("ABCDEFGH", "expired", 1_800_000);
  assert.deepEqual(lines, [
    "room ABCDEFGH created",
    "room ABCDEFGH player joined (1 connected)",
    "room ABCDEFGH player disconnected (code 1006) (0 connected)",
    "room ABCDEFGH player left (0 connected)",
    "room ABCDEFGH server closed a socket: 1008 Join timed out (0 connected)",
    "room ABCDEFGH ended: expired after 30m 0s",
  ]);
  assert.equal(monitor.sample().totals.roomsCreated, 1);
});
