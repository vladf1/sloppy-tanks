import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { gameServer } from "./servers.mjs";

const repo = new URL("..", import.meta.url);

function run(command, args) {
  const result = spawnSync(command, args, { cwd: repo, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

const info = JSON.parse(
  readFileSync(new URL("../dist-dev/health/index.html", import.meta.url), "utf8"),
);
// The dev site must always offer multiplayer. The link is compiled into the
// inline startup script only when the build has a multiplayer server URL.
const index = readFileSync(new URL("../dist-dev/index.html", import.meta.url), "utf8");
const assets = new URL("../dist-dev/assets/", import.meta.url);
const scripts = readdirSync(assets).filter((name) => name.endsWith(".js"));
// Code splitting may move the server address into a chunk that the room page shares with
// Battle Setup's room list, so look for it in any script beside the client entry.
const clientHasServer =
  scripts.some((name) => /^client-.*\.js$/.test(name)) &&
  scripts.some((name) =>
    readFileSync(new URL(name, assets), "utf8").includes(gameServer(true).url),
  );
if (!index.includes('id="tab-multiplayer"') || !clientHasServer) {
  throw new Error(
    "dist-dev has no multiplayer entry or dev server URL; rebuild with bun run build:dev",
  );
}

// Clients and the server reject each other unless both were built from the same
// game/network sources, so publish the dev server from this checkout first. It runs on
// its own machine, so this never touches production's. The deploy waits until the dev
// server reports this checkout's content version.
run(process.execPath, ["scripts/server.mjs", "deploy", "--dev"]);

const build = `${info.commit}${info.dirty ? ", local changes" : ""}`;
console.log(`Publishing dev build ${info.builtAt} (${build})`);
run(process.execPath, ["deploy/dev-site/precompress.mjs"]);
// Fixed destination: the Worker's account, name and domain come from its config.
run(process.execPath, [
  "tools/deploy/wrangler.mjs",
  "deploy",
  "--config",
  "deploy/dev-site/wrangler.jsonc",
  "--message",
  `Dev build ${info.builtAt} (${build})`,
]);
