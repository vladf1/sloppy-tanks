import { test } from "bun:test";
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
import { renderPage } from "../directory/page";

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

test("the overview lists every environment's servers and rooms, escaping what servers sent", () => {
  const now = 100_000;
  const page = renderPage(
    [
      {
        name: "production",
        servers: [
          {
            url: "wss://1-2-3-4.nip.io",
            version: "1.1.1.244",
            contentVersion: "11aac2382af5a93fd5d30531",
            serverBuild: "5a8e451fb56078b8714b650a",
            seen: now - 3000,
            rooms: [
              {
                room: "ABCDEFGH",
                mapMode: "<script>alert(1)</script>",
                players: 2,
                reserved: 3,
                phase: "playing",
                time: 125,
                scores: [4, 1],
                humansOnly: false,
                difficulty: "normal",
                roundMinutes: 10,
              },
            ],
          },
        ],
      },
      { name: "dev", servers: [] },
    ],
    now,
  );
  assert.match(page, /production <small>1 server · 1 room · 2 players<\/small>/);
  assert.match(page, /href="https:\/\/1-2-3-4\.nip\.io\/dashboard"/);
  assert.match(
    page,
    /v1\.1\.1\.244 · content 11aac238 · build 5a8e451f · 2 players · seen 3 s ago/,
  );
  assert.match(
    page,
    /<td>2\/8 \(\+1 reconnecting\)<\/td><td>playing · 2:05 left<\/td><td>4–1<\/td>/,
  );
  assert.match(page, /&lt;script&gt;alert\(1\)&lt;\/script&gt;/);
  assert.doesNotMatch(page, /<script>/);
  assert.match(
    page,
    /dev <small>0 servers · 0 rooms · 0 players<\/small><\/h2><p class="empty">No servers<\/p>/,
  );
});
