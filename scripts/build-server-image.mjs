import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { contentVersion, serverBuild } from "./content-version.mjs";

/** Build the multiplayer server's linux/amd64 Docker image from `Dockerfile`. The binary
 * comes from `scripts/build-server.mjs --vps`, built in the normal Cargo target directory
 * so its compiled dependencies are reused, and the image only copies it in. The image
 * labels carry the same content version and server build the binary stamps.
 *
 * Every image is tagged with its server build, so one build has one image wherever it
 * came from. Locally it is `sloppy-tanks-server:<build>` and `:latest`. CI passes
 * `--registry ghcr.io/vladf1/sloppy-tanks-server --push` and moving tags such as
 * `--tag pr-12`; when the registry already holds that build (a change outside the
 * server), it only adds the tags instead of rebuilding. */
const repo = fileURLToPath(new URL("..", import.meta.url));
const LOCAL_IMAGE = "sloppy-tanks-server";

function option(name) {
  const values = [];
  process.argv.forEach((arg, index) => {
    if (arg === name) values.push(process.argv[index + 1]);
  });
  return values;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: repo, stdio: "inherit", ...options });
  if (result.error) console.error(`${command}: ${result.error.message}`);
  return result;
}

function git(...args) {
  return spawnSync("git", args, { cwd: repo, encoding: "utf8" }).stdout.trim();
}

const name = option("--registry")[0] ?? LOCAL_IMAGE;
const push = process.argv.includes("--push");
const version = await contentVersion();
const build = await serverBuild();
const commit = git("rev-parse", "HEAD") + (git("status", "--porcelain") ? "-dirty" : "");
const image = `${name}:${build}`;
const extraTags = [...option("--tag"), ...(push ? [] : ["latest"])].map((tag) => `${name}:${tag}`);

const published =
  push &&
  run("docker", ["buildx", "imagetools", "inspect", image], { stdio: "ignore" }).status === 0;
if (published) {
  console.log(`${image} is already published; adding tags only`);
  if (extraTags.length > 0) {
    const tagArgs = extraTags.flatMap((tag) => ["--tag", tag]);
    const result = run("docker", ["buildx", "imagetools", "create", ...tagArgs, image]);
    if (result.status !== 0) process.exit(result.status ?? 1);
  }
} else {
  const binary = run("node", ["scripts/build-server.mjs", "--vps"]);
  if (binary.status !== 0) process.exit(binary.status ?? 1);
  const result = run("docker", [
    "build",
    "--platform",
    "linux/amd64",
    "--build-arg",
    `SLOPPY_CONTENT_VERSION=${version}`,
    "--build-arg",
    `SLOPPY_SERVER_BUILD=${build}`,
    "--build-arg",
    `GIT_COMMIT=${commit}`,
    ...[image, ...extraTags].flatMap((tag) => ["--tag", tag]),
    ...(push ? ["--push"] : []),
    ".",
  ]);
  if (result.status !== 0) process.exit(result.status ?? 1);
}
// CI reads the image name to promote it after the deploy gate.
if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `image=${image}\n`);
console.log(`Built ${image} (content ${version}, commit ${commit})`);
