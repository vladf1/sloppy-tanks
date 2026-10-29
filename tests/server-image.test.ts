import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { serverBuild } from "../scripts/content-version.mjs";
import { SERVER_IMAGE } from "../scripts/servers.mjs";

const script = fileURLToPath(new URL("../scripts/build-server-image.mjs", import.meta.url));

/** Runs build-server-image.mjs against a fake registry that holds `published`, with fake
 * `docker` and `node` (the binary's Cargo build) first on the PATH, and returns the docker
 * commands it ran, each split into its arguments. */
function buildImage(args: string[], published: string[]): string[][] {
  const bin = mkdtempSync(join(tmpdir(), "sloppy-server-image-"));
  try {
    const log = join(bin, "docker.log");
    writeFileSync(join(bin, "published"), published.map((ref) => `${ref}\n`).join(""));
    writeFileSync(
      join(bin, "docker"),
      [
        "#!/bin/sh",
        `echo "$*" >>"${log}"`,
        'if [ "$1 $2 $3" = "buildx imagetools inspect" ]; then',
        `  grep -qxF "$4" "${join(bin, "published")}" || exit 1`,
        `  echo '{"config":{"Labels":{}}}'`,
        "fi",
      ].join("\n"),
      { mode: 0o755 },
    );
    writeFileSync(join(bin, "node"), "#!/bin/sh\n", { mode: 0o755 });
    const result = spawnSync(process.execPath, [script, ...args], {
      encoding: "utf8",
      // CI's step output must not receive this run's image.
      env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, GITHUB_OUTPUT: "" },
    });
    assert.equal(result.status, 0, result.stderr);
    return readFileSync(log, "utf8")
      .trim()
      .split("\n")
      .map((line) => line.split(" "));
  } finally {
    rmSync(bin, { recursive: true, force: true });
  }
}

const tags = (command: string[]) =>
  command.flatMap((arg, index) => (command[index - 1] === "--tag" ? [arg] : []));
const devPublish = ["--registry", SERVER_IMAGE, "--push", "--dev", "--tag", "dev"];

test("a dev publish tags its build dev-<build>, never the bare build CI takes", async () => {
  const build = await serverBuild();
  const commands = buildImage(devPublish, []);
  const pushes = commands.filter((command) => command[0] === "build");
  assert.equal(pushes.length, 1);
  assert.deepEqual(tags(pushes[0]), [`${SERVER_IMAGE}:dev-${build}`, `${SERVER_IMAGE}:dev`]);
});

test("a dev publish points :dev at CI's image of the same server build", async () => {
  const build = await serverBuild();
  const commands = buildImage(devPublish, [`${SERVER_IMAGE}:${build}`]);
  assert.deepEqual(commands.at(-1), [
    "buildx",
    "imagetools",
    "create",
    "--tag",
    `${SERVER_IMAGE}:dev`,
    `${SERVER_IMAGE}:${build}`,
  ]);
  assert.ok(!commands.some((command) => command[0] === "build"));
});

test("CI builds its own image rather than adopting a dev publish of the build", async () => {
  const build = await serverBuild();
  const commands = buildImage(
    ["--registry", SERVER_IMAGE, "--push", "--tag", "main"],
    [`${SERVER_IMAGE}:dev-${build}`],
  );
  const pushes = commands.filter((command) => command[0] === "build");
  assert.equal(pushes.length, 1);
  assert.deepEqual(tags(pushes[0]), [`${SERVER_IMAGE}:${build}`, `${SERVER_IMAGE}:main`]);
});
