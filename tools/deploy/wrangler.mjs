import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repo = new URL("../..", import.meta.url);
function run(args) {
  const result = spawnSync(process.execPath, args, { cwd: repo, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

// A frozen install also reconciles dependency changes after switching branches.
run(["install", "--cwd", fileURLToPath(new URL(".", import.meta.url)), "--frozen-lockfile"]);
run([
  fileURLToPath(new URL("node_modules/wrangler/bin/wrangler.js", import.meta.url)),
  ...process.argv.slice(2),
]);
