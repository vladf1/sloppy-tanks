import { spawnSync } from "node:child_process";
import { contentVersion } from "./content-version.mjs";
import { VPS_MULTIPLAYER_URL, VPS_SSH } from "./vps-host.mjs";

const repo = new URL("..", import.meta.url);
const HEALTH_TIMEOUT_MS = 60000;
const SSH_OPTIONS = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=15"];

function run(command, args) {
  const result = spawnSync(command, args, { cwd: repo, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
const ssh = (script) => run("ssh", [...SSH_OPTIONS, VPS_SSH, script]);

// --provision installs Node, Caddy, the systemd unit and the environment file first.
if (process.argv.includes("--provision")) {
  console.log(`Provisioning ${VPS_SSH}`);
  ssh("rm -rf /root/sloppy-tanks-provision && mkdir -p /root/sloppy-tanks-provision");
  run("scp", [
    ...SSH_OPTIONS,
    "deploy/vps/provision.sh",
    "deploy/vps/Caddyfile",
    "deploy/vps/sloppy-tanks.service",
    "deploy/vps/sloppy-tanks.env",
    `${VPS_SSH}:/root/sloppy-tanks-provision/`,
  ]);
  ssh("bash /root/sloppy-tanks-provision/provision.sh");
}

const version = await contentVersion();
console.log(`Deploying Node multiplayer server (content ${version}) to ${VPS_SSH}`);
run("npm", ["run", "server:build"]);
run("scp", [
  ...SSH_OPTIONS,
  "server/dist/server.mjs",
  `${VPS_SSH}:/opt/sloppy-tanks/server.mjs.new`,
]);
run("scp", [
  ...SSH_OPTIONS,
  "server/dist/server.mjs.map",
  `${VPS_SSH}:/opt/sloppy-tanks/server.mjs.map.new`,
]);
// Rename in place so a crash-restart never loads a partially copied bundle. The restart
// resets live rooms; clients receive room-reset from the graceful shutdown.
ssh(
  "cd /opt/sloppy-tanks && mv -f server.mjs.map.new server.mjs.map && mv -f server.mjs.new server.mjs && systemctl restart sloppy-tanks",
);

const health = new URL("/health", VPS_MULTIPLAYER_URL.replace(/^ws/, "http"));
const deadline = Date.now() + HEALTH_TIMEOUT_MS;
for (;;) {
  const status = await fetch(health, { cache: "no-store" })
    .then((response) => response.json())
    .catch(() => ({}));
  if (status.contentVersion === version && status.multiplayerEnabled) break;
  if (Date.now() > deadline)
    throw new Error(`VPS server reports ${JSON.stringify(status)}; expected content ${version}`);
  await new Promise((resolve) => setTimeout(resolve, 2000));
}
console.log(`VPS multiplayer server is live at ${VPS_MULTIPLAYER_URL}`);
