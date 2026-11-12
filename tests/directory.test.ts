import { test } from "bun:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  ENVIRONMENTS,
  listServers,
  nipUrl,
  parseAnnouncement,
  parseEnvironment,
  readBody,
  type ListedServer,
} from "../directory/listing";
import type { RoomListing } from "../directory/room-listing";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

const room = (code: string, changes: Partial<RoomListing> = {}): RoomListing => ({
  room: code,
  contentVersion: "11aac2382af5a93fd5d30531",
  mapMode: "village",
  difficulty: "normal",
  humansOnly: false,
  roundMinutes: 10,
  players: 1,
  reserved: 1,
  phase: "lobby",
  roundId: 1,
  time: 0,
  scores: [0, 0],
  ...changes,
});
const health = {
  protocol: 4,
  contentVersion: "11aac2382af5a93fd5d30531",
  serverBuild: "5a8e451fb56078b8714b650a",
};
const server = (url: string, rooms: RoomListing[]): ListedServer => ({
  ...health,
  rooms,
  seen: 1,
  url,
});

test("the directory has one list per game server environment", () => {
  // The startup scripts name each machine's environment.
  const scripts = ["production", "dev", "app-store"].map(
    (name) =>
      read(`../deploy/server/cloud-init/${name}.yaml`).match(/SERVER_ENVIRONMENT=(\S+)/)?.[1],
  );
  assert.deepEqual([...ENVIRONMENTS].sort(), scripts.sort());
  assert.equal(parseEnvironment("production"), "production");
  assert.equal(parseEnvironment("dev"), "dev");
  assert.equal(parseEnvironment("app-store"), "app-store");
  for (const value of [null, "", "Production", "staging"]) {
    assert.equal(parseEnvironment(value), undefined);
  }
});

test("a server is listed at the nip.io name of the IPv4 address it announced from", () => {
  assert.equal(nipUrl("45.63.62.42"), "wss://45-63-62-42.nip.io");
  assert.equal(nipUrl("0.0.0.255"), "wss://0-0-0-255.nip.io");
  for (const ip of [
    null,
    "",
    "2001:db8::1",
    "::ffff:45.63.62.42",
    "45.63.62",
    "45.63.62.42.1",
    "45.63.62.256",
    "45.63.062.42",
    "45.63.62.-1",
    "45.63.62.4a",
    " 45.63.62.42",
  ]) {
    assert.equal(nipUrl(ip), undefined, String(ip));
  }
});

test("an announcement keeps its declared /health fields and rooms checked as the page checks them", () => {
  const body = {
    version: "1.2.3.4",
    ...health,
    dirty: false,
    startedAt: "2026-10-27T09:41:05.123Z",
    cpuPercent: 3.4,
    cpus: 1,
    rssMB: 41.2,
    hostMemoryMB: 961,
    rooms: [room("ABCDEFGH")],
  };
  const announced = parseAnnouncement(JSON.stringify({ ...body, url: "wss://elsewhere.example" }));
  assert.deepEqual(JSON.parse(JSON.stringify(announced)), body);
  const malformed = [
    {},
    { ...health },
    { ...health, rooms: {} },
    { ...health, rooms: [1] },
    { ...health, rooms: [{ room: "A" }] },
    { ...health, rooms: [room("ABCDEFGH", { players: 9 })] },
    { ...health, rooms: [room("ABCDEFGH", { mapMode: "<script>" as RoomListing["mapMode"] })] },
    { ...health, protocol: "4", rooms: [] },
    { ...health, contentVersion: "", rooms: [] },
    { ...health, version: 1, rooms: [] },
    { ...health, dirty: "no", rooms: [] },
    { ...health, startedAt: "yesterday", rooms: [] },
    { ...health, cpuPercent: -1, rooms: [] },
    { ...health, cpuPercent: "3", rooms: [] },
    { ...health, cpus: 1.5, rooms: [] },
    { ...health, rssMB: null, rooms: [] },
  ];
  // Servers deployed before the load figures announce without them.
  assert.deepEqual(
    JSON.parse(JSON.stringify(parseAnnouncement(JSON.stringify({ ...health, rooms: [] })))),
    { ...health, rooms: [] },
  );
  for (const text of [
    "",
    "not json",
    "null",
    "[]",
    '"rooms"',
    ...malformed.map((b) => JSON.stringify(b)),
  ]) {
    assert.equal(parseAnnouncement(text), undefined, text);
  }
});

/** A body of `chunks` chunks of `chunkBytes` bytes each that counts how many the reader
 * pulled and whether it cancelled; with a high-water mark of zero, a chunk is made only
 * when read. */
function countingBody(chunks: number, chunkBytes: number, headers: HeadersInit = {}) {
  const counts = { pulled: 0, cancelled: false };
  const body = new ReadableStream<Uint8Array>(
    {
      pull(controller) {
        if (counts.pulled === chunks) {
          controller.close();
          return;
        }
        controller.enqueue(new Uint8Array(chunkBytes).fill(++counts.pulled));
      },
      cancel() {
        counts.cancelled = true;
      },
    },
    { highWaterMark: 0 },
  );
  return { request: { headers: new Headers(headers), body }, counts };
}

test("a body under the limit is read whole", async () => {
  const { request, counts } = countingBody(3, 4);
  const body = await readBody(request, 12);
  assert.deepEqual([...(body ?? [])], [1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3]);
  assert.equal(counts.cancelled, false);
  assert.equal((await readBody({ headers: new Headers(), body: null }, 12))?.byteLength, 0);
});

test("a body over the limit stops being read one chunk past it", async () => {
  // Endless, as a sender that declares no length could make it.
  const { request, counts } = countingBody(Infinity, 4);
  assert.equal(await readBody(request, 10), undefined);
  // 8 bytes fit, the third chunk passes 10, and nothing more is pulled.
  assert.equal(counts.pulled, 3);
  assert.equal(counts.cancelled, true);
});

test("a declared Content-Length over the limit is refused unread", async () => {
  const { request, counts } = countingBody(1, 4, { "Content-Length": "11" });
  assert.equal(await readBody(request, 10), undefined);
  assert.equal(counts.pulled, 0);
});

test("servers are listed fewest rooms first, ties by url", () => {
  const servers = listServers([
    server("wss://9-9-9-9.nip.io", [room("AAAAAAAA"), room("BBBBBBBB")]),
    server("wss://5-5-5-5.nip.io", [room("CCCCCCCC")]),
    server("wss://3-3-3-3.nip.io", [room("DDDDDDDD")]),
    server("wss://7-7-7-7.nip.io", []),
  ]);
  assert.deepEqual(
    servers.map((listed) => listed.url),
    [
      "wss://7-7-7-7.nip.io",
      "wss://3-3-3-3.nip.io",
      "wss://5-5-5-5.nip.io",
      "wss://9-9-9-9.nip.io",
    ],
  );
});

test("the overview page has a section for every environment", () => {
  const page = read("../directory/public/index.html");
  const sections = [...page.matchAll(/data-environment="([^"]+)"/g)].map((match) => match[1]);
  assert.deepEqual(sections.sort(), [...ENVIRONMENTS].sort());
});
