import { test } from "bun:test";
import assert from "node:assert/strict";
import { swarmServer } from "../bots/regions";

const server = (url: string, ...rooms: string[]) => ({
  url,
  rooms: rooms.map((room) => ({ room })),
});

test("a swarm with a configured room plays on the server listing it", () => {
  const servers = [server("wss://a", "AAAAAAAA"), server("wss://b", "BBBBBBBB")];
  assert.equal(swarmServer(servers, undefined, "BBBBBBBB")?.url, "wss://b");
  assert.equal(swarmServer(servers, "wss://a", "BBBBBBBB")?.url, "wss://b");
});

test("without a listed room a swarm keeps its server, else takes the least busy", () => {
  const servers = [server("wss://a"), server("wss://b", "BBBBBBBB")];
  assert.equal(swarmServer(servers, "wss://b", "CCCCCCCC")?.url, "wss://b");
  assert.equal(swarmServer(servers, "wss://b", undefined)?.url, "wss://b");
  assert.equal(swarmServer(servers, "wss://gone", undefined)?.url, "wss://a");
  assert.equal(swarmServer(servers, undefined, "CCCCCCCC")?.url, "wss://a");
  assert.equal(swarmServer([], undefined, "CCCCCCCC"), undefined);
});
