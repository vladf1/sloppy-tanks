import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { DIRECTORY_URL } from "./servers.mjs";

const repo = new URL("..", import.meta.url);

function run(command, args) {
  const result = spawnSync(command, args, { cwd: repo, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

const info = JSON.parse(
  readFileSync(new URL("../dist-dev/health/index.html", import.meta.url), "utf8"),
);
// The dev site must always offer multiplayer. The tab is compiled into the inline
// startup script only when the build has a server directory URL.
const index = readFileSync(new URL("../dist-dev/index.html", import.meta.url), "utf8");
const assets = new URL("../dist-dev/assets/", import.meta.url);
// Code splitting decides which chunk carries the directory URL, so look in every script.
const hasDirectory = readdirSync(assets)
  .filter((name) => name.endsWith(".js"))
  .some((name) => readFileSync(new URL(name, assets), "utf8").includes(DIRECTORY_URL));
if (!index.includes('id="tab-multiplayer"') || !hasDirectory) {
  throw new Error(
    "dist-dev has no multiplayer entry or dev directory URL; rebuild with pnpm run build:dev",
  );
}

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
