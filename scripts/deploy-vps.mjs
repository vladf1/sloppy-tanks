import { spawn, spawnSync } from "node:child_process";
import { contentVersion, serverBuild } from "./content-version.mjs";
import { SERVER_IMAGE, VPS_SSH, VPS_SSH_OPTIONS, vpsHealth } from "./vps-host.mjs";

/** Build the server on this machine and deploy it over SSH: the dev server's normal
 * deploy, and production's fallback when CI or the registry cannot serve
 * (`pnpm run server:update` pulls CI's image instead). It builds the image with the local
 * Docker, loads it into Podman on the VPS and has deploy/vps/sloppy-tanks-update switch
 * to it, which sets a hold so auto-update does not replace it.
 * `--force` deploys production from a checkout that is not a clean origin/main. */
const repo = new URL("..", import.meta.url);
const HEALTH_TIMEOUT_MS = 60000;
const SSH_OPTIONS = ["-o", "BatchMode=yes", ...VPS_SSH_OPTIONS];

function run(command, args) {
  const result = spawnSync(command, args, { cwd: repo, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
const ssh = (script) => run("ssh", [...SSH_OPTIONS, VPS_SSH, script]);
const git = (...args) => spawnSync("git", args, { cwd: repo, encoding: "utf8" });

// --dev targets the dev site's server, a second service on the same host; production's
// unit, image and process are never touched by it.
const dev = process.argv.includes("--dev");
const target = dev
  ? {
      name: "dev",
      service: "sloppy-tanks-dev",
      updater: "dev",
    }
  : {
      name: "production",
      service: "sloppy-tanks",
      updater: "production",
    };

// Production runs what main published; a local build of anything else would stop
// matching the site's content version and send every visitor a reload message.
if (!dev && !process.argv.includes("--force")) {
  if (git("status", "--porcelain").stdout.trim()) {
    console.error("Production deploys need a clean checkout (or --force).");
    process.exit(1);
  }
  // Without GitHub the last fetched origin/main is the best reference there is.
  if (git("fetch", "--quiet", "origin", "main").status !== 0)
    console.warn("Could not fetch origin/main; comparing with the last fetched one.");
  const head = git("rev-parse", "HEAD").stdout.trim();
  if (head !== git("rev-parse", "origin/main").stdout.trim()) {
    console.error("Production deploys come from origin/main; check it out (or pass --force).");
    process.exit(1);
  }
}

// --provision installs Podman, Caddy, the updater, the Quadlet units and environment
// files first. With --dev it installs only Podman, the updater, the dev unit, its
// environment, Caddy's config and the 8443 firewall rule.
const provision = process.argv.includes("--provision");
if (provision) {
  console.log(`Provisioning the ${target.name} server on ${VPS_SSH}`);
  ssh("rm -rf /root/sloppy-tanks-provision && mkdir -p /root/sloppy-tanks-provision");
  const podman = [
    "install-podman.sh",
    "sloppy-tanks-update",
    "sloppy-tanks-update.service",
    "sloppy-tanks-update.timer",
  ];
  const devFiles = ["sloppy-tanks-dev.container", "sloppy-tanks-dev.env"];
  const files = dev
    ? ["provision-dev.sh", ...podman, "Caddyfile", ...devFiles]
    : [
        "provision.sh",
        "install-caddy.sh",
        ...podman,
        "Caddyfile",
        "sloppy-tanks.container",
        "sloppy-tanks.env",
        ...devFiles,
      ];
  run("scp", [
    ...SSH_OPTIONS,
    ...files.map((file) => `deploy/vps/${file}`),
    `${VPS_SSH}:/root/sloppy-tanks-provision/`,
  ]);
  ssh(`bash /root/sloppy-tanks-provision/${files[0]}`);
}

const version = await contentVersion();
const build = await serverBuild();
console.log(
  `Deploying the ${target.name} multiplayer server (content ${version}, build ${build}) to ${VPS_SSH}`,
);

/** Stream `docker save` here into `podman load` on the VPS. */
function loadImageOnServer(image) {
  return new Promise((resolve, reject) => {
    const save = spawn("docker", ["save", image], { stdio: ["ignore", "pipe", "inherit"] });
    const load = spawn("ssh", [...SSH_OPTIONS, "-C", VPS_SSH, "podman load"], {
      stdio: [save.stdout, "inherit", "inherit"],
    });
    let pending = 2;
    const done = (name) => (code) => {
      if (code !== 0) reject(new Error(`${name} exited with ${code}`));
      else if (--pending === 0) resolve();
    };
    // "exit", not "close": save's output belongs to the ssh process, so its streams
    // never close on this side.
    save.on("exit", done("docker save"));
    load.on("exit", done("podman load over SSH"));
  });
}

// Through Caddy, or on the machine itself before the public name reaches it.
const health = vpsHealth(dev);
const readHealth = health.read;
// The build, not only the content version, so a server-only change cannot pass against
// the previous process before a restart completes.
const isCurrent = (status) => status.contentVersion === version && status.serverBuild === build;

// Provisioning restarts a pinned service on its image. When that already is this build,
// a deploy would only restart it again and leave a hold.
if (provision) {
  const pinned = spawnSync("ssh", [
    ...SSH_OPTIONS,
    VPS_SSH,
    `test -f /var/lib/sloppy-tanks/${target.updater}.image`,
  ]);
  if (pinned.status === 0 && isCurrent(await readHealth())) {
    console.log(`The ${target.name} server already runs build ${build}`);
    process.exit(0);
  }
}

const image = `${SERVER_IMAGE}:${build}`;
run("node", ["scripts/build-server-image.mjs", "--registry", SERVER_IMAGE]);
await loadImageOnServer(image);
// Restarts the service and checks /health locally, rolling back if it fails. The restart
// resets live rooms; clients receive room-reset from the graceful shutdown.
ssh(`sloppy-tanks-update ${target.updater} local ${image}`);

const deadline = Date.now() + HEALTH_TIMEOUT_MS;
for (;;) {
  const status = await readHealth();
  if (isCurrent(status)) break;
  if (Date.now() > deadline)
    throw new Error(
      `VPS server reports ${JSON.stringify(status)}; expected content ${version}, build ${build}`,
    );
  await new Promise((resolve) => setTimeout(resolve, 2000));
}
console.log(`The ${target.name} multiplayer server is live at ${health.where}`);
