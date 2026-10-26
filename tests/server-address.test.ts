import { test } from "node:test";
import assert from "node:assert/strict";
import { gameServerAddress, resolveDirectoryAddress } from "../src/net/server-address";

const invalid = /Invalid server directory configuration/;
const directory = "https://directory.example/servers?environment=production";

test("the directory is the override, then the build's URL, then the local server", () => {
  assert.equal(
    resolveDirectoryAddress(null, undefined, "localhost")?.href,
    "http://127.0.0.1:8787/servers",
  );
  assert.equal(
    resolveDirectoryAddress(null, "", "127.0.0.1")?.href,
    "http://127.0.0.1:8787/servers",
  );
  assert.equal(resolveDirectoryAddress(null, undefined, "sloppy-tanks.example"), undefined);
  assert.equal(resolveDirectoryAddress(null, directory, "sloppy-tanks.example")?.href, directory);
  assert.equal(
    resolveDirectoryAddress("http://127.0.0.1:9000/servers", directory, "localhost")?.href,
    "http://127.0.0.1:9000/servers",
  );
  assert.equal(resolveDirectoryAddress(null, 42, "site.example"), undefined);
});

test("the directory must be https off a local page, without credentials or fragment", () => {
  assert.throws(
    () => resolveDirectoryAddress(null, "http://directory.example/servers", "site.example"),
    invalid,
  );
  assert.throws(() => resolveDirectoryAddress("https://a:b@b.example", null, "localhost"), invalid);
  assert.throws(() => resolveDirectoryAddress("https://b.example/#x", null, "localhost"), invalid);
  assert.throws(() => resolveDirectoryAddress("wss://b.example", null, "localhost"), invalid);
});

test("a listed game server must be a plain wss address off a local page", () => {
  assert.equal(gameServerAddress("wss://1-2-3-4.nip.io", "site.example"), "wss://1-2-3-4.nip.io");
  assert.equal(gameServerAddress("ws://127.0.0.1:8787/", "localhost"), "ws://127.0.0.1:8787");
  for (const url of [
    "ws://1-2-3-4.nip.io",
    "https://1-2-3-4.nip.io",
    "wss://a@b.example",
    "wss://b.example/?x",
    "wss://b.example/#x",
    "not a url",
    42,
  ]) {
    assert.throws(() => gameServerAddress(url, "site.example"), /Invalid game server address/);
  }
});
