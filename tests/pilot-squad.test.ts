import { test } from "bun:test";
import assert from "node:assert/strict";
import type {
  PilotAction,
  PilotEngine,
  PilotReport,
  PilotSetup,
  PilotStep,
} from "../bots/pilot-engine";
import {
  PilotSeat,
  type Connect,
  type PilotSocket,
  type PilotSocketEvents,
} from "../bots/pilot-seat";
import { PilotSquad, readBrain, type DirectoryServer } from "../bots/pilot-squad";

// The seat and squad around a stand-in engine; tests/pilots.test.ts plays the real one
// against the server.

const CONTENT = "test-content";
/** A pilot's first socket: generation 1 above the socket bits (`crates/pilot/src/pilot.rs`). */
const FIRST_SOCKET = (1 << 20) | 1;
const ROOM_FULL = { cause: "rejected", text: "This room has eight reserved player seats." };

/** Gives out handles from 1, as every new engine does, and plays the steps it is handed. */
class FakeEngine {
  readonly protocol = 7;
  readonly contentVersion = CONTENT;
  readonly pilots = new Map<number, { name: string; phase: string; step: PilotStep }>();
  /** Socket events the engine heard. */
  readonly heard: string[] = [];
  private next = 0;

  create(setup: PilotSetup): number {
    this.next++;
    this.pilots.set(this.next, {
      name: setup.name,
      phase: "connecting",
      step: { actions: [], ended: null },
    });
    return this.next;
  }

  tick(handle: number): PilotStep {
    const pilot = this.pilots.get(handle);
    if (!pilot) {
      return { actions: [], ended: null };
    }
    const step = pilot.step;
    pilot.step = { actions: [], ended: step.ended };
    return step;
  }

  leave(): PilotStep {
    return { actions: [], ended: null };
  }

  report(handle: number): PilotReport | null {
    const pilot = this.pilots.get(handle);
    return pilot ? ({ name: pilot.name, phase: pilot.phase } as PilotReport) : null;
  }

  remove(handle: number): void {
    this.pilots.delete(handle);
  }

  opened(handle: number, socket: number): void {
    this.heard.push(`opened ${handle} ${socket}`);
  }

  message(handle: number, socket: number): void {
    this.heard.push(`message ${handle} ${socket}`);
  }

  closed(handle: number, socket: number, code: number): void {
    this.heard.push(`closed ${handle} ${socket} ${code}`);
  }
}

const asEngine = (fake: FakeEngine) => fake as unknown as PilotEngine;

function fakeSocket(): PilotSocket & { sent: string[]; closed: boolean } {
  return {
    sent: [],
    closed: false,
    send(text) {
      this.sent.push(text);
    },
    close() {
      this.closed = true;
    },
  };
}

/** Lets the seat's connect callbacks run. */
const settle = () => new Promise((resolve) => setImmediate(resolve));

test("a pilot restarted in a new engine ignores the sockets of the old one", async () => {
  let engine = new FakeEngine();
  const opening: { events: PilotSocketEvents; resolve: (socket: PilotSocket) => void }[] = [];
  const connect: Connect = (_url, events) =>
    new Promise((resolve) => opening.push({ events, resolve }));
  const failures: unknown[] = [];
  const seat = new PilotSeat(
    {
      serverUrl: "wss://game.test",
      room: "ABCD2345",
      name: "pilot-wnam-1",
      orders: { mission: "skirmish", skill: "normal", kind: "balanced" },
      seed: 1,
    },
    { engine: () => asEngine(engine), now: () => 0, connect, failed: (e) => failures.push(e) },
  );
  const open: PilotAction = {
    type: "open",
    socket: FIRST_SOCKET,
    url: "wss://game.test/rooms/ABCD2345",
  };

  seat.start();
  engine.pilots.get(seat.handle)!.step = { actions: [open], ended: null };
  seat.tick();
  const old = fakeSocket();
  opening[0].resolve(old);
  await settle();
  assert.deepEqual(engine.heard, [`opened 1 ${FIRST_SOCKET}`]);

  // A trap: the squad starts the pilot in a new engine, which gives it the same handle and
  // the same socket id.
  seat.stop();
  assert.equal(old.closed, true);
  engine = new FakeEngine();
  seat.start();
  assert.equal(seat.handle, 1);
  engine.pilots.get(seat.handle)!.step = { actions: [open], ended: null };
  seat.tick();
  // The old socket's close and a late frame arrive while the new one is opening.
  opening[0].events.close(1006);
  opening[0].events.message("late");
  assert.deepEqual(engine.heard, []);

  const replacement = fakeSocket();
  opening[1].resolve(replacement);
  await settle();
  assert.equal(replacement.closed, false);
  assert.deepEqual(engine.heard, [`opened 1 ${FIRST_SOCKET}`]);
  engine.pilots.get(seat.handle)!.step = {
    actions: [{ type: "send", socket: FIRST_SOCKET, text: "input" }],
    ended: null,
  };
  seat.tick();
  assert.deepEqual(replacement.sent, ["input"]);
  assert.deepEqual(failures, []);
});

function squadOf(pilots: number, engine: FakeEngine, clock: { now: number }, room?: string) {
  return new PilotSquad(
    { region: "wnam", pilots, room, host: false, brain: readBrain({}) },
    () => asEngine(engine),
    () => new Promise(() => {}),
    () => clock.now,
  );
}

function server(...rooms: [code: string, reserved: number][]): DirectoryServer {
  return {
    url: "wss://game.test",
    contentVersion: CONTENT,
    rooms: rooms.map(([room, reserved]) => ({
      room,
      reserved,
      contentVersion: CONTENT,
      mapMode: "village",
    })),
  };
}

test("a room that turns one pilot away keeps the squad's other pilots playing there", () => {
  const engine = new FakeEngine();
  const clock = { now: 0 };
  const squad = squadOf(2, engine, clock, "ABCD2345");
  const servers = [server(["ABCD2345", 7])];
  squad.maintain(servers);
  const [seated, turnedAway] = squad.seats;
  engine.pilots.get(seated.handle)!.phase = "playing";
  engine.pilots.get(turnedAway.handle)!.step = { actions: [], ended: ROOM_FULL };
  squad.tick();

  clock.now += 15_000;
  squad.maintain(servers);
  assert.equal(squad.room, "ABCD2345");
  assert.equal(seated.handle, 1, "the seated pilot plays on");
  assert.equal(turnedAway.handle, 3, "the pilot turned away tries again");
  assert.equal(turnedAway.ended, undefined);

  // A room that turns every pilot away sends the squad to choose again.
  for (const seat of squad.seats) {
    engine.pilots.get(seat.handle)!.step = { actions: [], ended: ROOM_FULL };
  }
  squad.tick();
  clock.now += 15_000;
  squad.maintain(servers);
  assert.equal(squad.seats.length, 0);
  assert.match(squad.lastError ?? "", /eight reserved player seats/);
});

test("a squad joins the fullest open room with a seat for every pilot", () => {
  const squad = squadOf(3, new FakeEngine(), { now: 0 });
  squad.maintain([server(["FULL2345", 7], ["SOME2345", 5], ["EMPT2345", 0])]);
  assert.equal(squad.room, "SOME2345");

  const crowded = squadOf(3, new FakeEngine(), { now: 0 });
  crowded.maintain([server(["FULL2345", 7], ["MOST2345", 6])]);
  assert.equal(crowded.room, undefined);
  assert.match(crowded.lastError ?? "", /3 free seats/);
});
