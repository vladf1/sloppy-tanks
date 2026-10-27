import { test } from "node:test";
import assert from "node:assert/strict";
import { gameServerAddress, resolveDirectoryAddress } from "../src/net/server-address";

const invalid = /Invalid server directory configuration/;
const directory = "https://directory.example";
const page = (hostname: string, protocol = "https:") => new URL(`${protocol}//${hostname}/`);

test("the directory is the override, then the build's, then the local server", () => {
  const local = "http://127.0.0.1:8787/servers?environment=production";
  assert.equal(
    resolveDirectoryAddress(null, undefined, undefined, page("localhost", "http:"))?.href,
    local,
  );
  assert.equal(resolveDirectoryAddress(null, "", "", page("127.0.0.1"))?.href, local);
  assert.equal(
    resolveDirectoryAddress(null, undefined, "dev", page("sloppy-tanks.example")),
    undefined,
  );
  assert.equal(
    resolveDirectoryAddress(null, directory, "dev", page("sloppy-tanks.example"))?.href,
    "https://directory.example/servers?environment=dev",
  );
  assert.equal(
    resolveDirectoryAddress("http://127.0.0.1:9000", directory, "dev", page("localhost"))?.href,
    "http://127.0.0.1:9000/servers?environment=dev",
  );
  assert.equal(resolveDirectoryAddress(null, 42, undefined, page("site.example")), undefined);
});

test("the directory is a base address, https off a local page", () => {
  for (const [address, hostname] of [
    ["http://directory.example", "site.example"],
    ["https://a:b@b.example", "localhost"],
    ["https://b.example/?environment=dev", "localhost"],
    ["https://b.example/#x", "localhost"],
    ["wss://b.example", "localhost"],
  ]) {
    assert.throws(
      () => resolveDirectoryAddress(address, null, "dev", page(hostname)),
      invalid,
      address,
    );
  }
});

test("a configured path makes the page's own server its directory", () => {
  assert.equal(
    resolveDirectoryAddress(null, "/", undefined, new URL("https://play.example/?room=ABCD"))?.href,
    "https://play.example/servers?environment=production",
  );
  assert.equal(
    resolveDirectoryAddress(null, "/", undefined, new URL("http://127.0.0.1:9000/"))?.href,
    "http://127.0.0.1:9000/servers?environment=production",
  );
  assert.throws(
    () => resolveDirectoryAddress(null, "/", undefined, page("play.example", "http:")),
    invalid,
  );
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
