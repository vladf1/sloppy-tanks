import { test } from "node:test";
import assert from "node:assert/strict";
import { HISTORY_READINGS, RECENT_EVENTS, ServerMonitor } from "../server/monitor";
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
  ticks: 20,
  tickAvgMs: 2.5,
  tickMaxMs: 9,
  sentBytes: 1024 * 100,
  receivedBytes: 1024,
  sentMessages: { snapshot: 20, lobby: 1 },
  receivedMessages: { input: 30 },
};
const noWire = () => ({ sent: 0, received: 0 });
const idle = { samples: () => [], sockets: () => 0, wireBytes: noWire };

test("server monitor reads every second and sums ten readings into each /stats sample", () => {
  let second = 0;
  // Alternate a light and a heavy second so the sample must weight and take the worst.
  const monitor = new ServerMonitor(
    {
      samples: () => [{ ...room, tickAvgMs: second % 2 ? 4 : 2, tickMaxMs: second % 2 ? 12 : 3 }],
      sockets: () => 3,
      wireBytes: noWire,
    },
    () => {},
  );
  const heard: number[] = [];
  const unsubscribe = monitor.subscribe((reading) => heard.push(reading.tickMaxMs));
  for (; second < 9; second++)
    assert.equal(monitor.read().roomList[0].tickMaxMs, second % 2 ? 12 : 3);
  const stats = monitor.sample();
  assert.equal(stats.roomList[0].tickAvgMs, 3);
  assert.equal(stats.roomList[0].tickMaxMs, 12);
  assert.equal(stats.totals.sentMB, 1, "ten readings of 100 KB");
  assert.equal(heard.length, 10, "a sample is also a reading");
  assert.equal(monitor.history.length, 10);
  assert.equal("roomList" in monitor.history[0], false, "history keeps totals only");

  second = 10;
  assert.equal(monitor.sample().roomList[0].tickMaxMs, 3, "each sample starts a new window");
  unsubscribe();
  for (let reading = 0; reading < HISTORY_READINGS; reading++) monitor.read();
  assert.equal(heard.length, 11);
  assert.equal(monitor.history.length, HISTORY_READINGS);
});

test("server monitor adds message types across rooms and turns wire counters into rates", (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: 1_000_000 });
  let wire = { sent: 0, received: 0 };
  const other = { ...room, room: "IJKLMNOP", sentMessages: { snapshot: 20, pong: 1 } };
  const monitor = new ServerMonitor(
    {
      samples: () => [room, { ...other, receivedMessages: {} }],
      sockets: () => 6,
      wireBytes: () => wire,
    },
    () => {},
  );
  wire = { sent: 20 * 1024, received: 2 * 1024 };
  t.mock.timers.tick(1000);
  const reading = monitor.read();
  assert.deepEqual(reading.sentMessages, { snapshot: 40, lobby: 1, pong: 1 });
  assert.deepEqual(reading.receivedMessages, { input: 30 });
  assert.equal(reading.wireSentKBps, 20);
  assert.equal(reading.wireReceivedKBps, 2);
  assert.equal("sentMessages" in reading.roomList[0], false, "counts are server-wide only");
  wire = { sent: 30 * 1024, received: 2 * 1024 };
  t.mock.timers.tick(1000);
  assert.equal(monitor.read().wireSentKBps, 10, "readings take the difference of running totals");
});

test("server monitor keeps a bounded, numbered list of recent events", () => {
  const monitor = new ServerMonitor(idle, () => {});
  for (let event = 0; event < RECENT_EVENTS + 5; event++)
    monitor.activity("ABCDEFGH", { type: "joined", players: 1 });
  assert.equal(monitor.events.length, RECENT_EVENTS);
  assert.equal(monitor.events[0].id, 6);
  assert.equal(monitor.events.at(-1)!.id, RECENT_EVENTS + 5);
  assert.deepEqual(
    { room: monitor.events[0].room, message: monitor.events[0].message },
    { room: "ABCDEFGH", message: "player joined (1 connected)" },
  );
  assert.equal(monitor.totals().joins, RECENT_EVENTS + 5);
});

test("server monitor starts its event list with a startup line", () => {
  const lines: string[] = [];
  const monitor = new ServerMonitor(idle, (line) => lines.push(line));
  monitor.start();
  monitor.stop();
  assert.deepEqual(
    monitor.events.map(({ id, room, message }) => ({ id, room, message })),
    [{ id: 1, room: "", message: "server started" }],
  );
  assert.deepEqual(lines, [], "main.ts already logs the listener");
});

test("server monitor summarizes each minute while active, then logs one idle summary", () => {
  const lines: string[] = [];
  let rooms = [room];
  const monitor = new ServerMonitor(
    { samples: () => rooms, sockets: () => rooms.length * 3, wireBytes: noWire },
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
  const monitor = new ServerMonitor(idle, (line) => lines.push(line));
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
  // One plain line per lifecycle event, naming the room and the facts an operator needs.
  assert.equal(lines.length, 6);
  assert.ok(lines.every((line) => line.startsWith("room ABCDEFGH ")));
  for (const [i, fact] of [
    /created/,
    /joined.*1 connected/,
    /1006/,
    /left/,
    /1008.*Join timed out/,
    /expired.*30m/,
  ].entries())
    assert.match(lines[i], fact);
  assert.equal(monitor.sample().totals.roomsCreated, 1);
});
