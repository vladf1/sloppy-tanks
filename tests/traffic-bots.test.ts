import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import WebSocket from "ws";
import { BotPlayer, openSeats, randomRoomCode, type ServerInfo } from "../bots/bot-player";

// The traffic bots drive the real multiplayer server: `pnpm run server:build` (part of
// `pnpm run check`) builds this binary.
const SERVER = fileURLToPath(new URL("../target/server/sloppy-server", import.meta.url));
// One of the server's default local origins, as a Vite tab would send.
const ORIGIN = "http://127.0.0.1:5173";
const STEP_MS = 50;
const DRIVE_MS = 4000;
const START_TIMEOUT_MS = 10_000;
const ROOM_CODE = /^[A-Z2-9]{8}$/;

let server: ChildProcess | undefined;
let base = "";

before(async () => {
  assert.ok(existsSync(SERVER), `${SERVER} is missing: run pnpm run server:build first`);
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

/** A small seeded stream, so each bot's maneuvers repeat between runs. */
function seeded(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 2 ** 32;
  };
}

async function until(condition: () => boolean, what: string): Promise<void> {
  const deadline = performance.now() + START_TIMEOUT_MS;
  while (!condition()) {
    assert.ok(performance.now() < deadline, `Timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, STEP_MS));
  }
}

test("traffic bots create a room on the Rust server, drive with accepted input and stay connected", async () => {
  const health = (await (await fetch(`http://${base}/health`)).json()) as Record<string, unknown>;
  const info: ServerInfo = {
    version: Number(health.version),
    contentVersion: String(health.contentVersion),
  };
  const room = randomRoomCode();
  const bots = ["a", "b", "c"].map((id, index) => {
    const bot = new BotPlayer("bot-" + id, seeded(index + 7));
    return { bot, closed: undefined as number | undefined, acked: 0, socket: undefined as WebSocket | undefined };
  });
  for (const [index, seat] of bots.entries()) {
    const socket = new WebSocket(`ws://${base}/room/${room}`, { origin: ORIGIN });
    seat.socket = socket;
    socket.on("message", (data) => {
      const text = String(data);
      if (text.startsWith('{"type":"snapshot"')) {
        seat.acked = Math.max(seat.acked, (JSON.parse(text) as { ack: number }).ack);
      }
      seat.bot.receive(text, performance.now());
    });
    socket.on("close", (code) => {
      seat.closed = code;
      seat.bot.disconnected();
    });
    await new Promise((resolve, reject) => {
      socket.once("open", resolve);
      socket.once("error", reject);
    });
    seat.bot.join((text) => socket.send(text), info, performance.now(), index === 0);
    await until(() => seat.bot.phase !== "joining", `bot-${index} to join`);
  }
  assert.equal(bots[0].bot.phase, "playing", "The creating bot starts the first round");

  const timer = setInterval(() => bots.forEach(({ bot }) => bot.update(performance.now())), STEP_MS);
  await new Promise((resolve) => setTimeout(resolve, DRIVE_MS));
  clearInterval(timer);

  for (const { bot, closed, acked } of bots) {
    assert.equal(closed, undefined, "No bot was dropped for stale ticks or invalid messages");
    assert.equal(bot.lastError, undefined);
    assert.equal(bot.phase, "playing");
    assert.ok(bot.stats.inputs > 20, `Driving bots send active-rate input (${bot.stats.inputs})`);
    assert.ok(acked > 0, "The server applied the bot's input");
  }
  for (const { bot, socket } of bots) {
    bot.leave();
    socket!.close(1000);
  }
});

test("open seat planning fills occupied compatible rooms first and counts pending bots", () => {
  const rooms = [
    { room: "AAAAAAAA", contentVersion: "v1", reserved: 2 },
    { room: "BBBBBBBB", contentVersion: "v1", reserved: 6 },
    { room: "CCCCCCCC", contentVersion: "old", reserved: 1 },
    { room: "DDDDDDDD", contentVersion: "v1", reserved: 8 },
    { room: "EEEEEEEE", contentVersion: "v1", reserved: 3 },
  ];
  assert.deepEqual(openSeats(rooms, "v1", new Map([["BBBBBBBB", 2]]), new Set(["EEEEEEEE"])), [
    { room: "AAAAAAAA", free: 6 },
  ]);
  assert.deepEqual(
    openSeats(rooms, "v1", new Map(), new Set()).map((room) => room.room),
    ["BBBBBBBB", "EEEEEEEE", "AAAAAAAA"],
  );
  assert.match(randomRoomCode(), ROOM_CODE);
});
