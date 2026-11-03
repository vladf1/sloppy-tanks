import { spawnSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { delimiter } from "node:path";
import { fileURLToPath } from "node:url";

const repo = new URL("../..", import.meta.url);
function run(command, args) {
  const result = spawnSync(command, args, { cwd: repo, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

/** The oldest Node.js major version Wrangler supports. */
const NODE_MAJOR = 22;

// `bun run` puts a node -> bun shim first on PATH. Wrangler needs actual Node
// because Miniflare uses Undici's dispatcher, which Bun 1.4.4 does not implement.
// Take the first real Node new enough, so an old default (nvm's, say) fails here
// with instructions rather than inside Wrangler after the build.
const bun = realpathSync(process.execPath);
const nodes = (process.env.PATH ?? "")
  .split(delimiter)
  .map((directory) => Bun.which("node", { PATH: directory }))
  .filter((candidate) => candidate && realpathSync(candidate) !== bun)
  .map((path) => ({
    path,
    version: spawnSync(path, ["-p", "process.versions.node"], { encoding: "utf8" }).stdout?.trim(),
  }));
const node = nodes.find(({ version }) => Number(version?.split(".")[0]) >= NODE_MAJOR)?.path;
if (!node) {
  const found = nodes.map(({ path, version }) => `${version || "unknown"} at ${path}`);
  console.error(
    `Cloudflare tools require Node.js ${NODE_MAJOR} or newer on PATH` +
      (found.length ? ` (found ${found.join(", ")})` : "") +
      ". See tools/deploy/README.md.",
  );
  process.exit(1);
}

// A frozen install also reconciles dependency changes after switching branches.
run(process.execPath, [
  "install",
  "--cwd",
  fileURLToPath(new URL(".", import.meta.url)),
  "--frozen-lockfile",
]);
run(node, [
  fileURLToPath(new URL("node_modules/wrangler/bin/wrangler.js", import.meta.url)),
  ...process.argv.slice(2),
]);
