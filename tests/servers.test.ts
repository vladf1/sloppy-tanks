import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { gameServer, serverMachine } from "../scripts/servers.mjs";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

test("a machine with only a name made from its address is played and checked through it", () => {
  const server = serverMachine("dev", { ip: "45.63.56.58", hostnames: ["{dashed-ip}.nip.io"] });
  assert.deepEqual(server.sites, ["45-63-56-58.nip.io"]);
  assert.deepEqual(server.dnsNames, []);
  assert.equal(server.url, "wss://45-63-56-58.nip.io");
  assert.equal(server.checkUrl, "https://45-63-56-58.nip.io");
  assert.equal(server.ssh, "root@45.63.56.58");
});

test("players use the first hostname while the scripts check by the one made from the address", () => {
  const server = serverMachine("production", {
    ip: "45.63.56.58",
    hostnames: ["sloppy-tanks-server.fridman.me", "tanks.example.com", "{dashed-ip}.sslip.io"],
  });
  assert.deepEqual(server.sites, [
    "sloppy-tanks-server.fridman.me",
    "tanks.example.com",
    "45-63-56-58.sslip.io",
  ]);
  assert.deepEqual(server.dnsNames, ["sloppy-tanks-server.fridman.me", "tanks.example.com"]);
  assert.equal(server.url, "wss://sloppy-tanks-server.fridman.me");
  assert.equal(server.checkUrl, "https://45-63-56-58.sslip.io");
});

test("a local test machine without a name made from its address is checked by its hostname", () => {
  const server = serverMachine("production", {
    ip: "192.168.139.36",
    hostnames: ["sloppy-test.orb.local"],
  });
  assert.deepEqual(server.sites, ["sloppy-test.orb.local"]);
  assert.equal(server.checkUrl, "https://sloppy-test.orb.local");
});

test("a hostname with any placeholder but {dashed-ip} is refused", () => {
  assert.throws(
    () => serverMachine("dev", { ip: "45.63.56.58", hostnames: ["{ip}.nip.io"] }),
    /Only \{dashed-ip\}/,
  );
  assert.throws(() => serverMachine("dev", { ip: "45.63.56.58", hostnames: [] }), /no hostnames/);
});

test("the Pages build and the traffic bots use production's address from deploy/servers.json", () => {
  const { url } = gameServer(false);
  assert.equal(read("../.github/workflows/deploy.yml").match(/multiplayer-url: (\S+)/)?.[1], url);
  assert.equal(
    read("../bots/wrangler.jsonc").match(/"SERVER_URL": "([^"]+)"/)?.[1],
    url.replace(/^wss:/, "https:"),
  );
});
