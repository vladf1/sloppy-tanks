import { test } from "bun:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  addVersion,
  DIRECTORY_URL,
  formatVersions,
  KEPT_VERSIONS,
  machineAddress,
  option,
  parseVersions,
  SERVER_IMAGE,
  serverMachine,
  VERSIONS_LABEL,
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

test("a command line names a machine as --ip ADDRESS or --ip=ADDRESS, never as a bare --ip", () => {
  assert.equal(option(["rollback", "--ip", "1.2.3.4"], "--ip"), "1.2.3.4");
  assert.equal(option(["rollback", "--ip=1.2.3.4", "--dev"], "--ip"), "1.2.3.4");
  assert.equal(option(["rollback", "--dev"], "--ip"), undefined);
  // A flag left without its value would otherwise reach every listed machine.
  assert.throws(() => option(["rollback", "--ip"], "--ip"), /--ip needs a value/);
  assert.throws(() => option(["rollback", "--ip", "--dev"], "--ip"), /--ip needs a value/);
  assert.throws(() => option(["rollback", "--ip="], "--ip"), /--ip needs a value/);
  // Another flag that starts the same is not this one.
  assert.equal(option(["update", "--image-tag", "x"], "--image"), undefined);
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
  for (const name of ["production", "dev", "app-store"]) {
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

test("the machines run the versions the promote script lists, as many as it keeps", () => {
  const updater = read("../deploy/server/sloppy-tanks-update");
  assert.equal(updater.match(/^VERSIONS_LABEL=(\S+)$/m)?.[1], VERSIONS_LABEL);
  assert.equal(Number(updater.match(/^MAX_VERSIONS=(\d+)$/m)?.[1]), KEPT_VERSIONS);
  // Every environment follows the tag named after it, which only the promote script moves.
  assert.match(updater, /^follow=\$REGISTRY:\$\{environment:-production\}$/m);
  assert.ok(read("../.github/workflows/deploy.yml").includes("promote-server.mjs production "));
  assert.ok(read("../.github/workflows/desktop.yml").includes("promote-server.mjs app-store "));
  assert.ok(read("../scripts/server.mjs").includes('"scripts/promote-server.mjs", "dev"'));
});

test("a promotion puts the new version first, replaces an older image of it and keeps five", () => {
  const digest = (n: number) => `sha256:${String(n).repeat(64).slice(0, 64)}`;
  const version = (content: string, n: number) => ({ content, digest: digest(n) });
  const kept = ["aaaaaaaa", "bbbbbbbb", "cccccccc", "dddddddd", "eeeeeeee"].map(version);
  assert.deepEqual(addVersion(kept, version("ffffffff", 9)), [
    version("ffffffff", 9),
    ...kept.slice(0, 4),
  ]);
  // A server-only change of a kept version moves it to the front with its new image.
  assert.deepEqual(addVersion(kept, version("cccccccc", 7)), [
    version("cccccccc", 7),
    ...kept.filter(({ content }) => content !== "cccccccc"),
  ]);
  assert.deepEqual(addVersion([], version("aaaaaaaa", 1)), [version("aaaaaaaa", 1)]);
  assert.deepEqual(parseVersions(formatVersions(kept)), kept);
  assert.deepEqual(parseVersions(""), []);
  for (const label of [
    "aaaaaaaa",
    "AAAAAAAA@" + digest(1),
    "aaaaaaaa@sha256:12",
    "../x@" + digest(1),
  ]) {
    assert.throws(() => parseVersions(label), /Not a listed version/, label);
  }
});

test("a machine is listed once whatever versions it runs", () => {
  assert.equal(machineAddress("wss://45-63-56-58.nip.io"), "45.63.56.58");
  assert.equal(
    machineAddress("wss://45-63-56-58.nip.io/v/2d68366184e7ee8bd34ef6d9"),
    "45.63.56.58",
  );
  for (const url of [
    "ws://45-63-56-58.nip.io",
    "wss://example.com",
    "wss://45-63-56-58.nip.io/x",
  ]) {
    assert.throws(() => machineAddress(url), /Not a game server address/, url);
  }
});
