import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { once } from "node:events";
import RAPIER from "@dimforge/rapier3d-compat";
import WebSocket from "ws";
import { CONTENT_VERSION, PROTOCOL_VERSION } from "../src/net/protocol";
import { createServer, type MultiplayerServer } from "../server/server";

const ORIGIN = "http://127.0.0.1:5173";
let server: MultiplayerServer, base: string;
const logged: string[] = [];
before(async () => {
  await RAPIER.init();
  server = createServer({
    allowedOrigins: [ORIGIN],
    multiplayerEnabled: true,
    trustProxy: true,
    log: (line) => logged.push(line),
  });
  const address = await server.listen(0, "127.0.0.1");
  base = `127.0.0.1:${address.port}`;
});
after(() => server.close());

interface Message {
  type: string;
  reason?: string;
}
/** Opens a room socket and records every message and the close code it receives. */
async function connect(room: string, headers: Record<string, string> = {}) {
  const socket = new WebSocket(`ws://${base}/room/${room}`, { origin: ORIGIN, headers });
  const messages: Message[] = [];
  socket.on("message", (data: Buffer) => messages.push(JSON.parse(data.toString()) as Message));
  const closed = once(socket, "close").then(([code]) => code as number);
  await once(socket, "open");
  const next = async (type: string) => {
    while (!messages.some((message) => message.type === type))
      await new Promise((resolve) => setTimeout(resolve, 5));
    return messages.find((message) => message.type === type)!;
  };
  return { socket, messages, closed, next };
}
const join = (name: string) =>
  JSON.stringify({
    type: "join",
    version: PROTOCOL_VERSION,
    contentVersion: CONTENT_VERSION,
    name,
    kind: "balanced",
  });
const rooms = async (headers: Record<string, string> = { Origin: ORIGIN }) => {
  const response = await fetch(`http://${base}/rooms`, { headers });
  return { status: response.status, body: response.ok ? await response.json() : undefined };
};

test("node server reports health with the build's content version", async () => {
  const health = (await (await fetch(`http://${base}/health`)).json()) as Record<string, unknown>;
  assert.equal(health.contentVersion, CONTENT_VERSION);
  assert.equal(health.version, PROTOCOL_VERSION);
  assert.equal(health.multiplayerEnabled, true);
});

test("node server rejects foreign origins, plain HTTP rooms and invalid codes", async () => {
  assert.equal((await rooms({ Origin: "https://evil.example" })).status, 403);
  const plain = await fetch(`http://${base}/room/ABCDEFGH`, { headers: { Origin: ORIGIN } });
  assert.equal(plain.status, 426);
  const foreign = new WebSocket(`ws://${base}/room/ABCDEFGH`, { origin: "https://evil.example" });
  const [, refused] = (await once(foreign, "unexpected-response")) as [
    unknown,
    { statusCode: number },
  ];
  assert.equal(refused.statusCode, 403);
  const invalid = new WebSocket(`ws://${base}/room/abc`, { origin: ORIGIN });
  const [, missing] = (await once(invalid, "unexpected-response")) as [
    unknown,
    { statusCode: number },
  ];
  assert.equal(missing.statusCode, 404);
});

test("node server hosts a room, lists it and forgets it after the last leave", async () => {
  const player = await connect("TESTROOM");
  player.socket.send(join("player"));
  await player.next("welcome");
  assert.equal(server.rooms.has("TESTROOM"), true);
  const listed = (await rooms()).body as { rooms: { room: string; players: number }[] };
  assert.deepEqual(
    listed.rooms.map((room) => [room.room, room.players]),
    [["TESTROOM", 1]],
  );
  const stats = server.monitor.sample();
  assert.deepEqual(
    stats.roomList.map((room) => [room.room, room.players, room.sockets]),
    [["TESTROOM", 1, 1]],
  );
  assert.ok(stats.roomList[0].sentKBps > 0);
  player.socket.send(JSON.stringify({ type: "leave", roundId: 0 }));
  assert.equal(await player.closed, 1000);
  while (server.rooms.has("TESTROOM")) await new Promise((resolve) => setTimeout(resolve, 10));
  assert.deepEqual(((await rooms()).body as { rooms: unknown[] }).rooms, []);
  const lines = logged.filter((line) => line.startsWith("room TESTROOM"));
  assert.equal(lines[0], "room TESTROOM created");
  assert.equal(lines[1], "room TESTROOM player joined (1 connected)");
  assert.match(lines.at(-1)!, /^room TESTROOM ended: empty after \d+s$/);
});

test("node server serves /stats only to direct local requests", async () => {
  const direct = await fetch(`http://${base}/stats`);
  assert.equal(direct.status, 200);
  const stats = (await direct.json()) as Record<string, unknown>;
  assert.equal(typeof stats.rssMB, "number");
  assert.ok(Array.isArray(stats.roomList));
  const proxied = await fetch(`http://${base}/stats`, {
    headers: { "X-Forwarded-For": "203.0.113.7" },
  });
  assert.equal(proxied.status, 404);
});

test("node server rate-limits room connections per forwarded client IP", async () => {
  const statuses: number[] = [];
  for (let attempt = 0; attempt < 61; attempt++) {
    const socket = new WebSocket(`ws://${base}/room/RATELIMT`, {
      origin: ORIGIN,
      headers: { "X-Forwarded-For": "203.0.113.9" },
    });
    // A refused handshake also reports an error; the status is what this test checks.
    socket.on("error", () => {});
    const status = await new Promise<number>((resolve) => {
      socket.once("open", () => resolve(101));
      socket.once("unexpected-response", (_request, response) => resolve(response.statusCode!));
    });
    statuses.push(status);
    socket.terminate();
  }
  assert.equal(statuses.filter((status) => status === 101).length, 60);
  assert.equal(statuses.at(-1), 429);
});

test("node server shutdown resets live rooms", async () => {
  const player = await connect("SHUTDOWN", { "X-Forwarded-For": "198.51.100.4" });
  player.socket.send(join("player"));
  await player.next("welcome");
  await server.close();
  assert.equal(await player.closed, 1012);
  assert.equal((await player.next("room-reset")).reason, "server-restart");
  assert.equal(server.rooms.size, 0);
});
