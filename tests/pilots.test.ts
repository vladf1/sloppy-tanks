import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import WebSocket from "ws";
import { PilotEngine, type PilotSetup } from "../bots/pilot-engine";
import { PilotSeat, type Connect, type PilotHost } from "../bots/pilot-seat";
import { randomRoomCode } from "../bots/bot-player";

// The bots Worker's pilot engine against the real multiplayer server: `pnpm run
// pilot:build` and `pnpm run server:build` (both part of `pnpm run check:server`) build the
// Wasm and the binary, stamped with the same content version.
const SERVER_PATH =
  process.env.SLOPPY_SERVER_VPS === "1"
    ? "../target/x86_64-unknown-linux-musl/server/sloppy-server"
    : "../target/server/sloppy-server";
const SERVER = fileURLToPath(new URL(SERVER_PATH, import.meta.url));
const ENGINE = fileURLToPath(new URL("../bots/generated/sloppy_pilot.wasm", import.meta.url));
const FRAME_MS = 33;
const PLAY_TIMEOUT_MS = 20_000;
const FIGHT_TIMEOUT_MS = 30_000;
// PilotReport in bots/pilot-engine.ts.
const REPORT_KEYS = [
  "name",
  "room",
  "mission",
  "phase",
  "team",
  "host",
  "status",
  "notice",
  "ended",
  "endedText",
  "scoreKills",
  "scoreDeaths",
  "tally",
  "feel",
];
const TALLY_KEYS = [
  "rounds",
  "wins",
  "losses",
  "hits",
  "kills",
  "hitsTaken",
  "deaths",
  "playerHits",
  "playerHitsTaken",
  "triggerPulls",
  "teamChanges",
];
const FEEL_KEYS = [
  "rttMs",
  "fireToFlashMs",
  "fireToReportMs",
  "viewDelayMs",
  "hitOffsetM",
  "incomingOffsetM",
  "unseenIncoming",
  "predictedShots",
  "confirmedShots",
  "unconfirmedShots",
  "unpredictedShots",
  "phantomStrikes",
  "phantomIncoming",
  "corrections",
  "correctionP95M",
  "correctionMPerS",
  "predictionLeadMs",
  "lateBatches",
  "longestBatchGapMs",
];

let server: ChildProcess | undefined;
let base = "";
let module: WebAssembly.Module;

before(async () => {
  assert.ok(existsSync(SERVER), `${SERVER} is missing: run pnpm run server:build first`);
  assert.ok(existsSync(ENGINE), `${ENGINE} is missing: run pnpm run pilot:build first`);
  module = new WebAssembly.Module(readFileSync(ENGINE));
  const child = spawn(SERVER, [], {
    env: { ...process.env, HOST: "127.0.0.1", PORT: "0" },
    stdio: ["ignore", "pipe", "inherit"],
  });
  server = child;
  base = await new Promise<string>((resolve, reject) => {
    let output = "";
    child.once("exit", (code) => reject(new Error(`The server exited with ${code}`)));
    child.stdout!.on("data", (chunk: Buffer) => {
      output += chunk.toString();
      const address = /listening on (\S+)/.exec(output)?.[1];
      if (address) {
        resolve(`127.0.0.1:${address.split(":").at(-1)}`);
      }
    });
  });
});

after(() => {
  server?.kill("SIGTERM");
});

const connect: Connect = (url, events) =>
  new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    socket.binaryType = "arraybuffer";
    socket.once("open", () => resolve(socket));
    socket.once("error", reject);
    socket.on("message", (data: ArrayBuffer | Buffer, isBinary) =>
      events.message(isBinary ? new Uint8Array(data as ArrayBuffer) : String(data)),
    );
    socket.on("close", (code) => events.close(code));
  });

async function until(condition: () => boolean, what: string, timeoutMs: number): Promise<void> {
  const deadline = performance.now() + timeoutMs;
  while (!condition()) {
    assert.ok(performance.now() < deadline, `Timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, FRAME_MS));
  }
}

test("pilots in the Wasm engine create a room on the Rust server, fight there and leave", async () => {
  const engine = new PilotEngine(module);
  const health = (await (await fetch(`http://${base}/health`)).json()) as Record<string, unknown>;
  assert.equal(engine.protocol, Number(health.protocol));
  assert.equal(engine.contentVersion, String(health.contentVersion));
  const failures: unknown[] = [];
  const host: PilotHost = {
    engine: () => engine,
    now: () => performance.now(),
    connect,
    failed: (error) => failures.push(error),
  };
  const room = randomRoomCode();
  const setup = (name: string, mission: string, seed: number): Omit<PilotSetup, "savedSeat"> => ({
    serverUrl: `ws://${base}`,
    room,
    name,
    orders: { mission, skill: "hard", kind: "balanced" },
    seed,
  });
  const player = new PilotSeat(
    {
      ...setup("vlad", "skirmish", 1),
      create: { mapMode: "village", difficulty: "normal", humansOnly: false, roundMinutes: 5 },
    },
    host,
  );
  const hunter = new PilotSeat(setup("hunter", "hunt:vlad", 2), host);
  const seats = [player, hunter];
  player.start();
  const timer = setInterval(() => seats.forEach((seat) => seat.tick()), FRAME_MS);
  try {
    await until(
      () => player.report?.phase === "playing",
      "the room's maker to play",
      PLAY_TIMEOUT_MS,
    );
    hunter.start();
    await until(() => hunter.report?.phase === "playing", "the hunter to play", PLAY_TIMEOUT_MS);
    assert.ok(player.savedSeat, "the engine asked to keep the seat");
    // Both fight through the server: it fires their shots. (Whether a hunt finds its
    // player is the Rust tests' business: crates/pilot/tests/integration/missions.rs.)
    await until(
      () => seats.every((seat) => seat.report!.feel.confirmedShots > 0) || failures.length > 0,
      "the server to fire both pilots' shots",
      FIGHT_TIMEOUT_MS,
    );
    assert.deepEqual(failures, []);
    assert.notEqual(hunter.report!.team, player.report!.team, "the hunter took the other side");
    assert.equal(player.report!.host, true, "the room's maker hosts it");
    // The engine's report is the shape the control page reads.
    const keys = (value: object) => Object.keys(value).sort();
    assert.deepEqual(keys(hunter.report!), [...REPORT_KEYS].sort());
    assert.deepEqual(keys(hunter.report!.tally), [...TALLY_KEYS].sort());
    assert.deepEqual(keys(hunter.report!.feel), [...FEEL_KEYS].sort());
    for (const seat of seats) {
      assert.equal(seat.ended, undefined);
      assert.equal(seat.lastError, undefined);
      seat.leave();
    }
    await until(
      () => seats.every((seat) => seat.report?.phase === "left"),
      "both pilots to leave",
      PLAY_TIMEOUT_MS,
    );
  } finally {
    clearInterval(timer);
    seats.forEach((seat) => seat.stop());
  }
});

test("the engine refuses a bad setup and answers for pilots it does not have", () => {
  const engine = new PilotEngine(module);
  assert.throws(
    () =>
      engine.create(
        {
          serverUrl: "https://example.com",
          room: "ABCD2345",
          name: "p",
          orders: { mission: "skirmish", skill: "normal", kind: "balanced" },
          seed: 1,
        },
        0,
      ),
    /serverUrl must be a ws/,
  );
  // No pilot has this handle: the engine answers instead of trapping.
  assert.deepEqual(engine.tick(999, 0).actions, []);
  assert.equal(engine.report(999, 0), null);
});
