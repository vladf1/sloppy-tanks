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

// `bun run` puts a node -> bun shim first on PATH. Wrangler needs actual Node
// because Miniflare uses Undici's dispatcher, which Bun 1.4.4 does not implement.
const bun = realpathSync(process.execPath);
const node = (process.env.PATH ?? "")
  .split(delimiter)
  .map((directory) => Bun.which("node", { PATH: directory }))
  .find((candidate) => candidate && realpathSync(candidate) !== bun);
if (!node) {
  console.error(
    "Cloudflare tools require Node.js 22 or newer on PATH. See tools/deploy/README.md.",
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
