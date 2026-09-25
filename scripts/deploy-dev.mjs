import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { DEV_MULTIPLAYER_URL } from "./dev-multiplayer.mjs";

const repo = new URL("..", import.meta.url);
const env = { ...process.env, CLOUDFLARE_ACCOUNT_ID: "b49a59dfb5edf913223ad13eeab8d740" };

function run(command, args) {
  const result = spawnSync(command, args, { cwd: repo, stdio: "inherit", env });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

// Fixed destination: never infer a Pages project or branch from the checkout.
const info = JSON.parse(
  readFileSync(new URL("../dist-dev/build-info.json", import.meta.url), "utf8"),
);
// The dev site must always offer multiplayer. The link is compiled into the
// inline startup script only when the build has a multiplayer server URL.
const index = readFileSync(new URL("../dist-dev/index.html", import.meta.url), "utf8");
const assets = new URL("../dist-dev/assets/", import.meta.url);
const clientHasServer = readdirSync(assets).some(
  (name) =>
    /^client-.*\.js$/.test(name) &&
    readFileSync(new URL(name, assets), "utf8").includes(DEV_MULTIPLAYER_URL),
);
if (!index.includes("Play with friends") || !clientHasServer) {
  throw new Error(
    "dist-dev has no multiplayer entry or dev server URL; rebuild with npm run build:dev",
  );
}

// Clients and the server reject each other unless both were built from the same
// game/network sources, so publish the VPS server from this checkout first. The
// deploy waits until the VPS reports this checkout's content version.
run("node", ["scripts/deploy-vps.mjs"]);

console.log(
  `Publishing dev build ${info.builtAt} (${info.commit}${info.dirty ? ", local changes" : ""})`,
);
run("wrangler", [
  "pages",
  "deploy",
  "dist-dev",
  "--project-name",
  "sloppy-tanks-dev",
  "--branch",
  "main",
  "--commit-dirty=true",
]);
