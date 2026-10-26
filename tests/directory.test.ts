import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  ENTRY_TTL_MS,
  ENVIRONMENTS,
  expiredKeys,
  listServers,
  nipUrl,
  parseAnnouncement,
  parseEnvironment,
  type ServerEntry,
} from "../directory/listing";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

test("the directory has one list per game server environment", () => {
  const machines = JSON.parse(read("../deploy/servers.json")) as Record<string, unknown>;
  assert.deepEqual([...ENVIRONMENTS].sort(), Object.keys(machines).sort());
  assert.equal(parseEnvironment("production"), "production");
  assert.equal(parseEnvironment("dev"), "dev");
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

test("an announcement is a JSON object with a rooms array; its other fields pass through", () => {
  const body = { version: "1.2.3.4", protocol: 4, contentVersion: "abc", rooms: [{ room: "A" }] };
  assert.deepEqual(parseAnnouncement(JSON.stringify(body)), body);
  for (const text of ["", "not json", "null", "[]", '"rooms"', "{}", '{"rooms":{}}']) {
    assert.equal(parseAnnouncement(text), undefined, text);
  }
});

test("entries not announced within the time to live expire", () => {
  const now = 1_000_000;
  const entries = new Map<string, ServerEntry>([
    ["wss://1-1-1-1.nip.io", { rooms: [], seen: now }],
    ["wss://2-2-2-2.nip.io", { rooms: [], seen: now - ENTRY_TTL_MS }],
    ["wss://3-3-3-3.nip.io", { rooms: [], seen: now - ENTRY_TTL_MS - 1 }],
  ]);
  assert.deepEqual(expiredKeys(entries, now), ["wss://3-3-3-3.nip.io"]);
});

test("servers are listed fewest rooms first, ties by url, at the url they announced from", () => {
  const entries = new Map<string, ServerEntry>([
    ["wss://9-9-9-9.nip.io", { rooms: [1, 2], seen: 1 }],
    ["wss://5-5-5-5.nip.io", { rooms: [1], seen: 2 }],
    ["wss://3-3-3-3.nip.io", { rooms: [1], seen: 3, url: "wss://elsewhere.example" }],
    ["wss://7-7-7-7.nip.io", { rooms: [], seen: 4, protocol: 4 }],
  ]);
  const servers = listServers(entries);
  assert.deepEqual(
    servers.map((server) => server.url),
    [
      "wss://7-7-7-7.nip.io",
      "wss://3-3-3-3.nip.io",
      "wss://5-5-5-5.nip.io",
      "wss://9-9-9-9.nip.io",
    ],
  );
  assert.deepEqual(servers[0], { url: "wss://7-7-7-7.nip.io", rooms: [], seen: 4, protocol: 4 });
});
