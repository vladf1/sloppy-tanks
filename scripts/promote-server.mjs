import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { deploymentTag, formatDeployment, registry } from "./deployment.mjs";
import {
  addVersion,
  formatVersions,
  parseVersions,
  SERVER_IMAGE,
  VERSIONS_LABEL,
} from "./servers.mjs";

/** Publish a server image to an environment's machines:
 *
 *   bun scripts/promote-server.mjs production|dev|app-store IMAGE
 *
 * The machines read their environment's deployment file (scripts/deployment.mjs) and run a
 * server for every content version it lists (deploy/server/sloppy-tanks-update). This puts
 * IMAGE first in that list, in place of an older image of the same content version (a
 * server-only change), and drops the oldest beyond KEPT_VERSIONS. The deploy workflow runs
 * it for production after the site deploys, the Apps workflow for app-store once both
 * TestFlight uploads succeed, and `server:publish --dev` for dev. Needs Docker logged in to
 * ghcr.io and a token that can write packages (GITHUB_TOKEN, else the GitHub CLI's). */
const repo = fileURLToPath(new URL("..", import.meta.url));
const ENVIRONMENTS = ["production", "dev", "app-store"];
const LABELS = "me.fridman.sloppy-tanks";
/** A promotion reads the file back this long after writing it, longer than any promotion
 * takes from its read to its write: the registry has no conditional write, so a
 * promotion that wrote meanwhile from an older read shows here and this one tries again. */
const SETTLE_MS = 5000;
const ATTEMPTS = 5;

function docker(args, options = {}) {
  return spawnSync("docker", args, { cwd: repo, encoding: "utf8", ...options });
}

/** The labels and root digest of an image in the registry, or nothing when it holds none.
 * Any other failure stops the promotion: read as an empty list, a registry hiccup would push
 * a deployment that drops every version kept so far. */
function inspect(ref) {
  const result = docker([
    "buildx",
    "imagetools",
    "inspect",
    ref,
    "--format",
    '{"manifest":{{json .Manifest}},"image":{{json .Image}}}',
  ]);
  if (result.status !== 0) {
    if (/: not found\s*$/.test(result.stderr)) return undefined;
    console.error(`Could not read ${ref}:\n${result.stderr}`);
    process.exit(1);
  }
  const { manifest, image } = JSON.parse(result.stdout);
  // A manifest list maps each platform to its config; the image has only linux/amd64.
  const labels = (image.config ?? image["linux/amd64"]?.config)?.Labels ?? {};
  return { digest: manifest.digest, labels };
}

/** A deployed version of the server image `digest` names, with the stamps its labels
 * carry, which the file keeps for the people who read it. */
function deployedVersion(digest, labels) {
  return {
    contentVersion: labels[`${LABELS}.content-version`],
    digest,
    release: labels[`${LABELS}.release`],
    serverBuild: labels[`${LABELS}.server-build`],
  };
}

/** The versions the environment's tag listed in its label before the deployment file
 * existed: the first promotion's starting point. */
function labelledVersions(tag) {
  const current = inspect(tag);
  if (!current) return [];
  const listed = current.labels[VERSIONS_LABEL];
  if (listed === undefined) return [deployedVersion(current.digest, current.labels)];
  return parseVersions(listed).map(({ digest }) => {
    const image = inspect(`${SERVER_IMAGE}@${digest}`);
    if (!image) {
      console.error(`${tag} lists ${digest}, which the registry no longer holds`);
      process.exit(1);
    }
    return deployedVersion(digest, image.labels);
  });
}

/** Points the environment's image tag at the root version's image, relabelled with the
 * list, for machines created before the deployment file. Only the label changes, so the
 * image shares the root image's layers. A relabel is a new image every time, and those
 * machines restart for any new image: it pushes only when the list changes. */
function relabel(tag, versions) {
  const listed = formatVersions(versions);
  if (inspect(tag)?.labels[VERSIONS_LABEL] === listed) return;
  const dockerfile = [
    `FROM ${SERVER_IMAGE}@${versions[0].digest}`,
    `LABEL ${VERSIONS_LABEL}="${listed}"`,
  ].join("\n");
  const built = docker(
    ["build", "--platform", "linux/amd64", "--pull", "--tag", tag, "--push", "-"],
    { input: `${dockerfile}\n`, stdio: ["pipe", "inherit", "inherit"] },
  );
  if (built.status !== 0) process.exit(built.status ?? 1);
}

const [environment, source] = process.argv.slice(2);
if (!ENVIRONMENTS.includes(environment) || !source) {
  console.error(`Usage: bun scripts/promote-server.mjs ${ENVIRONMENTS.join("|")} IMAGE`);
  process.exit(2);
}
const image = inspect(source);
if (!image) {
  console.error(`The registry has no ${source}`);
  process.exit(1);
}
const added = deployedVersion(image.digest, image.labels);
if (!added.contentVersion) {
  console.error(`${source} has no content version label; is it a server image?`);
  process.exit(1);
}
const servers = registry(SERVER_IMAGE);
const tag = deploymentTag(environment);
const listed = (versions) =>
  versions.some(
    ({ contentVersion, digest }) =>
      contentVersion === added.contentVersion && digest === added.digest,
  );

let deployment;
for (let attempt = 1; ; attempt++) {
  const current = await servers.read(tag);
  const kept = current?.versions ?? labelledVersions(`${SERVER_IMAGE}:${environment}`);
  const versions = addVersion(kept, added);
  if (current && formatDeployment({ versions }) === formatDeployment(current)) {
    console.log(`${SERVER_IMAGE}:${tag} already lists ${source} first; nothing to push`);
    deployment = current;
    break;
  }
  await servers.write(tag, { versions });
  await new Promise((resolve) => setTimeout(resolve, SETTLE_MS));
  deployment = await servers.read(tag);
  if (deployment && listed(deployment.versions)) break;
  if (attempt === ATTEMPTS) {
    console.error(`Another promotion kept replacing ${tag}; ${source} is not listed`);
    process.exit(1);
  }
  console.log(`Another promotion wrote ${tag} meanwhile; trying again`);
}
console.log(`${SERVER_IMAGE}:${tag} lists:\n${formatDeployment(deployment)}`);
// Another promotion may write the file while this one relabels: the label follows the
// file as it is once relabelled, so both kinds of machine end on the same list.
for (;;) {
  relabel(`${SERVER_IMAGE}:${environment}`, deployment.versions);
  const latest = await servers.read(tag);
  if (formatDeployment(latest) === formatDeployment(deployment)) break;
  deployment = latest;
}
