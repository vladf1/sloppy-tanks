import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { gameServer, nipName, serverMachine } from "../scripts/servers.mjs";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

test("a machine without a hostname is reached and played through its nip.io name", () => {
  const server = serverMachine("dev", { ip: "45.63.56.58", hostname: null, settings: {} });
  assert.equal(nipName("45.63.56.58"), "45-63-56-58.nip.io");
  assert.deepEqual(server.sites, ["45-63-56-58.nip.io"]);
  assert.equal(server.url, "wss://45-63-56-58.nip.io");
  assert.equal(server.checkUrl, "https://45-63-56-58.nip.io");
  assert.equal(server.ssh, "root@45.63.56.58");
});

test("players use the hostname while the scripts check the machine by its nip.io name", () => {
  const server = serverMachine("production", {
    ip: "45.63.56.58",
    hostname: "sloppy-tanks-server.fridman.me",
    settings: {},
  });
  assert.deepEqual(server.sites, ["sloppy-tanks-server.fridman.me", "45-63-56-58.nip.io"]);
  assert.equal(server.url, "wss://sloppy-tanks-server.fridman.me");
  assert.equal(server.checkUrl, "https://45-63-56-58.nip.io");
});

test("a machine still to be created has an address but no SSH destination", () => {
  const server = serverMachine("production", {
    ip: null,
    hostname: "sloppy-tanks-server.fridman.me",
    settings: {},
  });
  assert.equal(server.url, "wss://sloppy-tanks-server.fridman.me");
  assert.throws(() => server.ssh, /no ip yet/);
  assert.throws(
    () => serverMachine("dev", { ip: null, hostname: null, settings: {} }),
    /needs an ip/,
  );
});

test("a local test machine gets no nip.io name, which no certificate authority could reach", () => {
  const server = serverMachine("production", {
    ip: "192.168.139.36",
    hostname: "sloppy-test.orb.local",
    settings: {},
  });
  assert.deepEqual(server.sites, ["sloppy-test.orb.local"]);
  assert.equal(server.checkUrl, "https://sloppy-test.orb.local");
});

test("the Pages build and the traffic bots use production's address from deploy/servers.json", () => {
  const { url } = gameServer(false);
  assert.equal(read("../.github/workflows/deploy.yml").match(/multiplayer-url: (\S+)/)?.[1], url);
  assert.equal(
    read("../bots/wrangler.jsonc").match(/"SERVER_URL": "([^"]+)"/)?.[1],
    url.replace(/^wss:/, "https:"),
  );
});
