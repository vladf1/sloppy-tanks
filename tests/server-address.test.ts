import { test } from "node:test";
import assert from "node:assert/strict";
import { resolveServerAddress } from "../src/net/server-address";

const invalid = /Invalid multiplayer server configuration/;

test("server address prefers the override, then the build URL, then the local server", () => {
  assert.equal(resolveServerAddress(null, undefined, "localhost")?.href, "ws://127.0.0.1:8787/");
  assert.equal(resolveServerAddress(null, "", "127.0.0.1")?.href, "ws://127.0.0.1:8787/");
  assert.equal(resolveServerAddress(null, undefined, "sloppy-tanks.example"), undefined);
  assert.equal(
    resolveServerAddress(null, "wss://play.example/", "sloppy-tanks.example")?.href,
    "wss://play.example/",
  );
  assert.equal(
    resolveServerAddress("ws://127.0.0.1:9000", "wss://play.example", "localhost")?.href,
    "ws://127.0.0.1:9000/",
  );
  assert.equal(resolveServerAddress(null, 42, "site.example"), undefined);
});

test("server address rejects insecure, credentialed and decorated endpoints", () => {
  assert.throws(() => resolveServerAddress(null, "ws://play.example", "site.example"), invalid);
  assert.throws(() => resolveServerAddress("wss://a:b@b.example", null, "localhost"), invalid);
  assert.throws(() => resolveServerAddress("wss://a@b.example", null, "localhost"), invalid);
  assert.throws(() => resolveServerAddress("wss://b.example/?x", null, "localhost"), invalid);
  assert.throws(() => resolveServerAddress("wss://b.example/#x", null, "localhost"), invalid);
  assert.throws(() => resolveServerAddress("https://b.example", null, "localhost"), invalid);
});
