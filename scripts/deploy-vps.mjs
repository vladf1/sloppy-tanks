import { spawnSync } from "node:child_process";
import { contentVersion, serverBuild } from "./content-version.mjs";
import { VPS_MULTIPLAYER_URL, VPS_SSH, VPS_SSH_OPTIONS } from "./vps-host.mjs";

const repo = new URL("..", import.meta.url);
const HEALTH_TIMEOUT_MS = 60000;
const SSH_OPTIONS = ["-o", "BatchMode=yes", ...VPS_SSH_OPTIONS];

function run(command, args) {
  const result = spawnSync(command, args, { cwd: repo, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
const ssh = (script) => run("ssh", [...SSH_OPTIONS, VPS_SSH, script]);

// --provision installs Caddy, the service user, the systemd unit and the environment file first.
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
const build = await serverBuild();
console.log(`Deploying the multiplayer server (content ${version}, build ${build}) to ${VPS_SSH}`);
// A static musl binary: the host needs no runtime, only the file.
run("node", ["scripts/build-server.mjs", "--vps"]);
run("scp", [
  ...SSH_OPTIONS,
  "target/x86_64-unknown-linux-musl/server/sloppy-server",
  `${VPS_SSH}:/opt/sloppy-tanks/sloppy-server.new`,
]);
// Rename in place so a crash-restart never runs a partially copied binary. The restart
// resets live rooms; clients receive room-reset from the graceful shutdown.
ssh(
  "cd /opt/sloppy-tanks && chmod 755 sloppy-server.new && mv -f sloppy-server.new sloppy-server && systemctl restart sloppy-tanks",
);

const health = new URL("/health", VPS_MULTIPLAYER_URL.replace(/^ws/, "http"));
const deadline = Date.now() + HEALTH_TIMEOUT_MS;
for (;;) {
  const status = await fetch(health, { cache: "no-store" })
    .then((response) => response.json())
    .catch(() => ({}));
  // The build, not only the content version, so a server-only change cannot pass
  // against the previous process before the restart completes.
  if (status.contentVersion === version && status.serverBuild === build) break;
  if (Date.now() > deadline)
    throw new Error(
      `VPS server reports ${JSON.stringify(status)}; expected content ${version}, build ${build}`,
    );
  await new Promise((resolve) => setTimeout(resolve, 2000));
}
console.log(`VPS multiplayer server is live at ${VPS_MULTIPLAYER_URL}`);
