import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { DIRECTORY_URL, SERVER_IMAGE, serverMachine } from "../scripts/servers.mjs";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

test("a machine is reached over SSH at its address and checked through its nip.io name", () => {
  const server = serverMachine("dev", "45.63.56.58");
  assert.equal(server.site, "45-63-56-58.nip.io");
  assert.equal(server.url, "wss://45-63-56-58.nip.io");
  assert.equal(server.checkUrl, "https://45-63-56-58.nip.io");
  assert.equal(server.ssh, "root@45.63.56.58");
  assert.throws(() => serverMachine("dev", "45-63-56-58.nip.io"), /Not an IPv4 address/);
});

test("the production build, the traffic bots, the pilots and the startup scripts use one server directory", () => {
  assert.equal(
    read("../.github/workflows/deploy.yml").match(/directory-url: (\S+)/)?.[1],
    DIRECTORY_URL,
  );
  for (const config of ["wrangler.jsonc", "pilots.wrangler.jsonc"]) {
    assert.equal(read(`../bots/${config}`).match(/"DIRECTORY_URL": "([^"]+)"/)?.[1], DIRECTORY_URL);
  }
  assert.equal(
    read("../crates/pilot/src/command_line.rs").match(/DEFAULT_DIRECTORY: &str = "([^"]+)"/)?.[1],
    DIRECTORY_URL,
  );
  for (const name of ["production", "dev"]) {
    assert.equal(
      read(`../deploy/server/cloud-init/${name}.yaml`).match(/DIRECTORY_URL=(\S+)/)?.[1],
      DIRECTORY_URL,
    );
  }
});

test("the machines pull the image CI and the dev publish push", () => {
  assert.equal(
    read("../deploy/server/sloppy-tanks-update").match(/^REGISTRY=(\S+)$/m)?.[1],
    SERVER_IMAGE,
  );
  assert.equal(
    read("../.github/workflows/check.yml").match(/SERVER_IMAGE: (\S+)/)?.[1],
    SERVER_IMAGE,
  );
});
