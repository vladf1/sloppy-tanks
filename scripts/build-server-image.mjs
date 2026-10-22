import { spawnSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { contentVersion, imageBuild, serverBuild, siteBuild } from "./content-version.mjs";
import { isNumberedRelease, releaseVersion } from "./release-version.mjs";
import { buildServerSite, SERVER_SITE_DIRECTORY } from "./server-site.mjs";

/** Build the multiplayer server's linux/amd64 Docker image from `Dockerfile`. The binary
 * comes from `scripts/build-server.mjs --vps`, built in the normal Cargo target directory
 * so its compiled dependencies are reused, and the game page it serves from
 * `scripts/server-site.mjs` (which needs `pnpm run wasm` first); the image only copies
 * both in. CI passes `--prebuilt` with both already in place from its other jobs. The
 * image labels carry the same content version and server build the binary stamps, and
 * the site build `/health` reports for the page.
 *
 * Every image is tagged with its server build and site build, so one build has one image
 * wherever it came from. Locally it is `sloppy-tanks-server:<server>-<site>` and
 * `:latest`. CI passes `--registry ghcr.io/vladf1/sloppy-tanks-server --push` and moving
 * tags such as `--tag pr-12`; when the registry already holds that build (a change that
 * reaches neither the server nor its page), it only adds the tags instead of rebuilding.
 *
 * Main's builds also pass `--build-number`, the Pages workflow's run number, which
 * completes the release version (`1.1.0.628`, scripts/release-version.mjs) the image
 * records as a label and as `SLOPPY_RELEASE` for `/health`. A server build keeps the
 * release of the first main build that shipped it: a pull request usually built the
 * image first, without a number, so that main build restamps the image's metadata
 * (release, commit and time) over the same layers, and later builds only retag it.
 * The VPS restarts only when :production's image changes, which then happens once per
 * server and page build. */
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

const RELEASE_LABEL = "me.fridman.sloppy-tanks.release";
const name = option("--registry")[0] ?? LOCAL_IMAGE;
const release = releaseVersion(option("--build-number")[0]);
const push = process.argv.includes("--push");
const prebuilt = process.argv.includes("--prebuilt");
const version = await contentVersion();
const build = await serverBuild();
const site = prebuilt ? await siteBuild(SERVER_SITE_DIRECTORY) : await buildServerSite();
const commit = git("rev-parse", "HEAD") + (git("status", "--porcelain") ? "-dirty" : "");
const image = `${name}:${imageBuild(build, site)}`;
const extraTags = [...option("--tag"), ...(push ? [] : ["latest"])].map((tag) => `${name}:${tag}`);

const builtAt = new Date().toISOString();

/** The published image's labels, or `undefined` when the registry does not hold it. */
function publishedLabels() {
  const result = spawnSync(
    "docker",
    ["buildx", "imagetools", "inspect", image, "--format", "{{json .Image}}"],
    { cwd: repo, encoding: "utf8" },
  );
  if (result.status !== 0) return undefined;
  const config = JSON.parse(result.stdout);
  // A manifest list maps each platform to its config; the image has only linux/amd64.
  return (config.config ?? config["linux/amd64"]?.config)?.Labels ?? {};
}

const labels = push ? publishedLabels() : undefined;
const published = labels !== undefined;
if (published && isNumberedRelease(release) && !isNumberedRelease(labels[RELEASE_LABEL])) {
  console.log(`${image} has no build number yet; stamping release ${release}`);
  // Only metadata changes: the new image shares the published binary layer.
  const dockerfile = [
    `FROM ${image}`,
    `LABEL org.opencontainers.image.revision=${commit} ${RELEASE_LABEL}=${release}`,
    `ENV SLOPPY_COMMIT=${commit} SLOPPY_BUILT_AT=${builtAt} SLOPPY_RELEASE=${release}`,
  ].join("\n");
  const result = run(
    "docker",
    [
      "build",
      "--platform",
      "linux/amd64",
      "--pull",
      ...[image, ...extraTags].flatMap((tag) => ["--tag", tag]),
      "--push",
      "-",
    ],
    { input: `${dockerfile}\n`, stdio: ["pipe", "inherit", "inherit"] },
  );
  if (result.status !== 0) process.exit(result.status ?? 1);
} else if (published) {
  console.log(`${image} is already published; adding tags only`);
  if (extraTags.length > 0) {
    const tagArgs = extraTags.flatMap((tag) => ["--tag", tag]);
    const result = run("docker", ["buildx", "imagetools", "create", ...tagArgs, image]);
    if (result.status !== 0) process.exit(result.status ?? 1);
  }
} else {
  if (!prebuilt) {
    const binary = run("node", ["scripts/build-server.mjs", "--vps"]);
    if (binary.status !== 0) process.exit(binary.status ?? 1);
  }
  const result = run("docker", [
    "build",
    "--platform",
    "linux/amd64",
    "--build-arg",
    `SLOPPY_CONTENT_VERSION=${version}`,
    "--build-arg",
    `SLOPPY_SERVER_BUILD=${build}`,
    "--build-arg",
    `SLOPPY_SITE_BUILD=${site}`,
    "--build-arg",
    `GIT_COMMIT=${commit}`,
    "--build-arg",
    `BUILT_AT=${builtAt}`,
    "--build-arg",
    `RELEASE=${release}`,
    ...[image, ...extraTags].flatMap((tag) => ["--tag", tag]),
    ...(push ? ["--push"] : []),
    ".",
  ]);
  if (result.status !== 0) process.exit(result.status ?? 1);
}
// CI reads the image name to promote it after the deploy gate.
if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `image=${image}\n`);
console.log(
  `Built ${image} (release ${release}, content ${version}, site ${site}, commit ${commit})`,
);
