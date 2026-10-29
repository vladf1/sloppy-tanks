import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  DEV_SERVER_IMAGE,
  DIRECTORY_URL,
  SERVER_IMAGE,
  serverMachine,
} from "../scripts/servers.mjs";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

test("a machine is reached over SSH at its address and checked through its nip.io name", () => {
  const server = serverMachine("dev", "45.63.56.58");
  assert.equal(server.site, "45-63-56-58.nip.io");
  assert.equal(server.url, "wss://45-63-56-58.nip.io");
  assert.equal(server.checkUrl, "https://45-63-56-58.nip.io");
  assert.equal(server.ssh, "root@45.63.56.58");
  assert.throws(() => serverMachine("dev", "45-63-56-58.nip.io"), /Not an IPv4 address/);
});

test("the Pages build, the traffic bots and the startup scripts use one server directory", () => {
  assert.equal(
    read("../.github/workflows/deploy.yml").match(/directory-url: (\S+)/)?.[1],
    DIRECTORY_URL,
  );
  assert.equal(
    read("../bots/wrangler.jsonc").match(/"DIRECTORY_URL": "([^"]+)"/)?.[1],
    DIRECTORY_URL,
  );
  for (const name of ["production", "dev"]) {
    assert.equal(
      read(`../deploy/server/cloud-init/${name}.yaml`).match(/DIRECTORY_URL=(\S+)/)?.[1],
      DIRECTORY_URL,
    );
  }
});

test("the machines pull from the packages the scripts push to, dev's apart from CI's", () => {
  const updater = read("../deploy/server/sloppy-tanks-update");
  assert.equal(updater.match(/^REGISTRY=(\S+)$/m)?.[1], SERVER_IMAGE);
  assert.equal(updater.match(/^DEV_REGISTRY=(\S+)$/m)?.[1], DEV_SERVER_IMAGE);
  assert.notEqual(DEV_SERVER_IMAGE, SERVER_IMAGE);
  assert.equal(
    read("../.github/workflows/check.yml").match(/SERVER_IMAGE: (\S+)/)?.[1],
    SERVER_IMAGE,
  );
});
