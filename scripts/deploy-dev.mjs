import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const repo = new URL("..", import.meta.url);

function run(command, args) {
  const result = spawnSync(command, args, { cwd: repo, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

const info = JSON.parse(
  readFileSync(new URL("../dist-dev/health/index.html", import.meta.url), "utf8"),
);

// Clients and the server reject each other unless both were built from the same
// game/network sources, so publish the dev server from this checkout first. It runs on
// its own machines, which follow the server image's :dev tag, so this never touches
// production's. The publish waits until every dev server reports this checkout's build.
run("node", ["scripts/server.mjs", "publish", "--dev"]);

const build = `${info.commit}${info.dirty ? ", local changes" : ""}`;
console.log(`Publishing dev build ${info.builtAt} (${build})`);
run("node", ["deploy/site/precompress.mjs", "dist-dev"]);
// Fixed destination: the Worker's account, name and domain come from its config.
run("wrangler", [
  "deploy",
  "--config",
  "deploy/site/dev.wrangler.jsonc",
  "--message",
  `Dev build ${info.builtAt} (${build})`,
]);
