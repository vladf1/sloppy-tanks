import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
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
 * The machines follow the tag named after their environment and run a server for every
 * content version its image's VERSIONS_LABEL lists (deploy/server/sloppy-tanks-update).
 * This puts IMAGE first in that list, in place of an older image of the same content
 * version (a server-only change), drops the oldest beyond KEPT_VERSIONS, and points the
 * tag at IMAGE restamped with the new list: only the label changes, so the image shares
 * IMAGE's layers and machines that predate the list run it as before. The deploy workflow
 * runs it for production after the site deploys, the Apps workflow for app-store once
 * both TestFlight uploads succeed, and `server:publish --dev` for dev. Needs Docker logged
 * in to ghcr.io with a token that can write packages. */
const repo = fileURLToPath(new URL("..", import.meta.url));
const ENVIRONMENTS = ["production", "dev", "app-store"];
const CONTENT_LABEL = "me.fridman.sloppy-tanks.content-version";

function docker(args, options = {}) {
  return spawnSync("docker", args, { cwd: repo, encoding: "utf8", ...options });
}

/** The labels and root digest of an image in the registry, or nothing when it holds none. */
function inspect(ref) {
  const result = docker([
    "buildx",
    "imagetools",
    "inspect",
    ref,
    "--format",
    '{"manifest":{{json .Manifest}},"image":{{json .Image}}}',
  ]);
  if (result.status !== 0) return undefined;
  const { manifest, image } = JSON.parse(result.stdout);
  // A manifest list maps each platform to its config; the image has only linux/amd64.
  const labels = (image.config ?? image["linux/amd64"]?.config)?.Labels ?? {};
  return { digest: manifest.digest, labels };
}

const [environment, source] = process.argv.slice(2);
if (!ENVIRONMENTS.includes(environment) || !source) {
  console.error(`Usage: bun scripts/promote-server.mjs ${ENVIRONMENTS.join("|")} IMAGE`);
  process.exit(2);
}
const tag = `${SERVER_IMAGE}:${environment}`;
const added = inspect(source);
if (!added) {
  console.error(`The registry has no ${source}`);
  process.exit(1);
}
const content = added.labels[CONTENT_LABEL];
if (!content) {
  console.error(`${source} has no content version label; is it a server image?`);
  process.exit(1);
}
// The list so far; a tag pushed before the list existed stands for its own image alone.
const current = inspect(tag);
const kept = !current
  ? []
  : current.labels[VERSIONS_LABEL] !== undefined
    ? parseVersions(current.labels[VERSIONS_LABEL])
    : [{ content: current.labels[CONTENT_LABEL], digest: current.digest }];
const versions = addVersion(kept, { content, digest: added.digest });
console.log(`${tag} keeps:\n${versions.map((v) => `  ${v.content} ${v.digest}`).join("\n")}`);
const dockerfile = [
  `FROM ${SERVER_IMAGE}@${added.digest}`,
  `LABEL ${VERSIONS_LABEL}="${formatVersions(versions)}"`,
].join("\n");
const built = docker(
  ["build", "--platform", "linux/amd64", "--pull", "--tag", tag, "--push", "-"],
  { input: `${dockerfile}\n`, stdio: ["pipe", "inherit", "inherit"] },
);
if (built.status !== 0) process.exit(built.status ?? 1);
console.log(`${tag} now serves ${source} (content ${content}) first`);
