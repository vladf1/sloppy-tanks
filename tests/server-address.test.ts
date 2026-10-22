import { test } from "node:test";
import assert from "node:assert/strict";
import { resolveServerAddress } from "../src/net/server-address";

const invalid = /Invalid multiplayer server configuration/;
const page = (hostname: string, protocol = "https:") => new URL(`${protocol}//${hostname}/`);

test("server address prefers the override, then the build URL, then the local server", () => {
  assert.equal(
    resolveServerAddress(null, undefined, page("localhost", "http:"))?.href,
    "ws://127.0.0.1:8787/",
  );
  assert.equal(resolveServerAddress(null, "", page("127.0.0.1"))?.href, "ws://127.0.0.1:8787/");
  assert.equal(resolveServerAddress(null, undefined, page("sloppy-tanks.example")), undefined);
  assert.equal(
    resolveServerAddress(null, "wss://play.example/", page("sloppy-tanks.example"))?.href,
    "wss://play.example/",
  );
  assert.equal(
    resolveServerAddress("ws://127.0.0.1:9000", "wss://play.example", page("localhost"))?.href,
    "ws://127.0.0.1:9000/",
  );
  assert.equal(resolveServerAddress(null, 42, page("site.example")), undefined);
});

test("a configured path names the server that serves the page", () => {
  assert.equal(
    resolveServerAddress(null, "/", new URL("https://play.example/?room=ABCD"))?.href,
    "wss://play.example/",
  );
  assert.equal(
    resolveServerAddress(null, "/", new URL("http://127.0.0.1:9000/"))?.href,
    "ws://127.0.0.1:9000/",
  );
  assert.throws(() => resolveServerAddress(null, "/", page("play.example", "http:")), invalid);
});

test("server address rejects insecure, credentialed and decorated endpoints", () => {
  assert.throws(
    () => resolveServerAddress(null, "ws://play.example", page("site.example")),
    invalid,
  );
  assert.throws(
    () => resolveServerAddress("wss://a:b@b.example", null, page("localhost")),
    invalid,
  );
  assert.throws(() => resolveServerAddress("wss://a@b.example", null, page("localhost")), invalid);
  assert.throws(() => resolveServerAddress("wss://b.example/?x", null, page("localhost")), invalid);
  assert.throws(() => resolveServerAddress("wss://b.example/#x", null, page("localhost")), invalid);
  assert.throws(() => resolveServerAddress("https://b.example", null, page("localhost")), invalid);
});
