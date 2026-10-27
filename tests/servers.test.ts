import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { DIRECTORY_URL, serverMachine } from "../scripts/servers.mjs";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

test("a machine with only a name made from its address is played and checked through it", () => {
  const server = serverMachine("dev", { ip: "45.63.56.58", hostnames: ["{dashed-ip}.nip.io"] });
  assert.deepEqual(server.sites, ["45-63-56-58.nip.io"]);
  assert.deepEqual(server.dnsNames, []);
  assert.equal(server.url, "wss://45-63-56-58.nip.io");
  assert.equal(server.checkUrl, "https://45-63-56-58.nip.io");
  assert.equal(server.ssh, "root@45.63.56.58");
  assert.equal(server.directory, "https://sloppy-tanks-directory.fridman.me");
});

test("players use the first hostname while the scripts check by the one made from the address", () => {
  const server = serverMachine("production", {
    ip: "45.63.56.58",
    hostnames: ["play.example.com", "tanks.example.com", "{dashed-ip}.sslip.io"],
  });
  assert.deepEqual(server.sites, ["play.example.com", "tanks.example.com", "45-63-56-58.sslip.io"]);
  assert.deepEqual(server.dnsNames, ["play.example.com", "tanks.example.com"]);
  assert.equal(server.url, "wss://play.example.com");
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

test("the Pages build and the traffic bots read the server directory", () => {
  assert.equal(
    read("../.github/workflows/deploy.yml").match(/directory-url: (\S+)/)?.[1],
    DIRECTORY_URL,
  );
  assert.equal(
    read("../bots/wrangler.jsonc").match(/"DIRECTORY_URL": "([^"]+)"/)?.[1],
    DIRECTORY_URL,
  );
});
