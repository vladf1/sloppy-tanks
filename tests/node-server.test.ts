import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { once } from "node:events";
import RAPIER from "@dimforge/rapier3d-compat";
import WebSocket from "ws";
import { CONTENT_VERSION, PROTOCOL_VERSION } from "../src/net/protocol";
import { MAX_DASHBOARD_VIEWERS } from "../server/dashboard";
import { createServer, type MultiplayerServer } from "../server/server";

const ORIGIN = "http://127.0.0.1:5173";
let server: MultiplayerServer, base: string;
const logged: string[] = [];
before(async () => {
  await RAPIER.init();
  server = createServer({
    allowedOrigins: [ORIGIN],
    trustProxy: true,
    // The rate-limit test opens sockets faster than their closes are counted.
    maxSocketsPerIp: 1000,
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
const join = (name: string, extra: object = {}) =>
  JSON.stringify({
    type: "join",
    version: PROTOCOL_VERSION,
    contentVersion: CONTENT_VERSION,
    name,
    kind: "balanced",
    ...extra,
  });
const rooms = async (headers: Record<string, string> = { Origin: ORIGIN }, query = "") => {
  const response = await fetch(`http://${base}/rooms${query}`, { headers });
  return { status: response.status, body: response.ok ? await response.json() : undefined };
};

test("node server reports health with the build's content version", async () => {
  const health = (await (await fetch(`http://${base}/health`)).json()) as Record<string, unknown>;
  assert.equal(health.contentVersion, CONTENT_VERSION);
  assert.equal(health.version, PROTOCOL_VERSION);
  assert.deepEqual(Object.keys(health).sort(), ["contentVersion", "version"]);
});

test("node server root returns the same pretty-printed body as /health", async () => {
  const [root, health] = await Promise.all([
    fetch(`http://${base}/`),
    fetch(`http://${base}/health`),
  ]);
  assert.equal(root.status, 200);
  assert.equal(root.headers.get("content-type"), "application/json");
  const body = await root.text();
  assert.equal(body, await health.text());
  assert.equal(body, JSON.stringify(JSON.parse(body), null, 2) + "\n");
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
  assert.match(player.socket.extensions, /permessage-deflate/, "room traffic is compressed");
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

test("extra-level rooms are listed only when asked for, and the plain list is unchanged", async () => {
  const player = await connect("YARDROOM");
  player.socket.send(
    join("yard", {
      create: { mapMode: "superstress", difficulty: "normal", humansOnly: false, roundMinutes: 5 },
    }),
  );
  await player.next("welcome");
  const codes = async (query: string) =>
    ((await rooms(undefined, query)).body as { rooms: { room: string }[] }).rooms.map(
      (room) => room.room,
    );
  assert.deepEqual(await codes(""), []);
  assert.deepEqual(await codes("?extralevels"), ["YARDROOM"]);
  player.socket.send(JSON.stringify({ type: "leave", roundId: 1 }));
  await player.closed;
  while (server.rooms.has("YARDROOM")) await new Promise((resolve) => setTimeout(resolve, 10));
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

/** Opens /dashboard/stream and reads its Server-Sent Events one at a time. */
async function openDashboard(ip: string) {
  const controller = new AbortController();
  const response = await fetch(`http://${base}/dashboard/stream`, {
    headers: { "X-Forwarded-For": ip },
    signal: controller.signal,
  });
  const reader = response.body!.pipeThrough(new TextDecoderStream()).getReader();
  let buffered = "";
  const next = async () => {
    while (!buffered.includes("\n\n")) {
      const { value, done } = await reader.read();
      if (done) throw new Error("Dashboard stream ended");
      buffered += value;
    }
    const end = buffered.indexOf("\n\n"),
      text = buffered.slice(0, end);
    buffered = buffered.slice(end + 2);
    return {
      text,
      type: /^event: (.+)$/m.exec(text)?.[1],
      data: JSON.parse(/^data: (.+)$/m.exec(text)![1]) as Record<string, unknown>,
    };
  };
  return { status: response.status, next, close: () => controller.abort() };
}

test("node server streams the dashboard without revealing room codes", async () => {
  const page = await fetch(`http://${base}/dashboard`);
  assert.equal(page.status, 200);
  assert.match(page.headers.get("content-type")!, /^text\/html/);
  assert.match(page.headers.get("content-security-policy")!, /frame-ancestors 'none'/);
  assert.match(await page.text(), /<title>Sloppy Tanks server<\/title>/);

  const player = await connect("DASHROOM");
  player.socket.send(join("player"));
  await player.next("welcome");
  const viewer = await openDashboard("198.51.100.20");
  assert.equal(viewer.status, 200);
  const hello = await viewer.next();
  assert.equal(hello.type, "hello");
  assert.equal((hello.data.server as { contentVersion: string }).contentVersion, CONTENT_VERSION);
  assert.ok(Array.isArray(hello.data.history));
  server.monitor.read();
  const reading = await viewer.next();
  assert.equal(reading.type, "reading");
  assert.deepEqual(
    (reading.data.roomList as { room: string; players: number }[]).map((room) => [
      room.room,
      room.players,
    ]),
    [["DAS•••••", 1]],
  );
  for (const { text } of [hello, reading]) assert.doesNotMatch(text, /DASHROOM/);
  const points = server.monitor.history;
  assert.ok(
    points.some((point) => point.wireSentKBps > 0),
    "socket bytes are counted",
  );
  assert.ok(points.some((point) => (point.receivedMessages.join ?? 0) > 0));
  assert.ok(points.some((point) => (point.sentMessages.welcome ?? 0) > 0));
  assert.match(hello.text + reading.text, /DAS•••••/, "events and rooms show the masked code");
  viewer.close();
  player.socket.send(JSON.stringify({ type: "leave", roundId: 0 }));
  await player.closed;
  while (server.rooms.has("DASHROOM")) await new Promise((resolve) => setTimeout(resolve, 10));
});

test("node server caps dashboard viewers and frees a slot when one leaves", async () => {
  const ip = "198.51.100.21";
  const viewers = [];
  for (let viewer = 0; viewer < MAX_DASHBOARD_VIEWERS; viewer++) {
    viewers.push(await openDashboard(ip));
    assert.equal((await viewers.at(-1)!.next()).type, "hello");
  }
  const refused = await openDashboard(ip);
  assert.equal(refused.status, 503);
  refused.close();
  viewers.pop()!.close();
  let reopened = await openDashboard(ip);
  // The server notices the closed stream a moment after the client aborts it.
  for (let attempt = 0; reopened.status === 503 && attempt < 10; attempt++) {
    reopened.close();
    await new Promise((resolve) => setTimeout(resolve, 20));
    reopened = await openDashboard(ip);
  }
  assert.equal(reopened.status, 200);
  for (const viewer of [...viewers, reopened]) viewer.close();
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

test("node server caps live rooms and open sockets per address", async () => {
  const small = createServer({
    allowedOrigins: [ORIGIN],
    trustProxy: true,
    maxRooms: 1,
    maxSocketsPerIp: 2,
    log: () => {},
  });
  const { port } = await small.listen(0, "127.0.0.1");
  const open = (room: string, ip: string) =>
    new Promise<{ status: number; socket: WebSocket }>((resolve) => {
      const socket = new WebSocket(`ws://127.0.0.1:${port}/room/${room}`, {
        origin: ORIGIN,
        headers: { "X-Forwarded-For": ip },
      });
      socket.on("error", () => {});
      socket.once("open", () => resolve({ status: 101, socket }));
      socket.once("unexpected-response", (_request, response) =>
        resolve({ status: response.statusCode!, socket }),
      );
    });
  try {
    const first = await open("CAPROOM2", "192.0.2.1");
    assert.equal(first.status, 101);
    assert.equal((await open("CAPROOM3", "192.0.2.2")).status, 503, "a new room past the cap");
    const second = await open("CAPROOM2", "192.0.2.1");
    assert.equal(second.status, 101, "joining an existing room is still allowed");
    assert.equal((await open("CAPROOM2", "192.0.2.1")).status, 429, "third socket from one IP");
    second.socket.close();
    await once(second.socket, "close");
    await new Promise((resolve) => setTimeout(resolve, 20));
    const again = await open("CAPROOM2", "192.0.2.1");
    assert.equal(again.status, 101, "a closed socket frees its address slot");
  } finally {
    await small.close();
  }
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
