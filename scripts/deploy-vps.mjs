import { spawn, spawnSync } from "node:child_process";
import { contentVersion, serverBuild } from "./content-version.mjs";
import {
  SERVER_IMAGE,
  VPS_DEV_MULTIPLAYER_URL,
  VPS_MULTIPLAYER_URL,
  VPS_SSH,
  VPS_SSH_OPTIONS,
} from "./vps-host.mjs";

/** Build the server on this machine and deploy it over SSH: the dev server's normal
 * deploy, and production's fallback when CI or the registry cannot serve
 * (`pnpm run server:update` pulls CI's image instead). It builds the image with the local
 * Docker, loads it into the VPS's container runtime (Podman, or Docker as the fallback)
 * and has deploy/vps/sloppy-tanks-update switch to it, which sets a hold so auto-update
 * does not replace it. A service still on the former binary unit gets the static binary
 * as before, until provisioning moves it to a container.
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
      dir: "/opt/sloppy-tanks-dev",
      url: VPS_DEV_MULTIPLAYER_URL,
    }
  : {
      name: "production",
      service: "sloppy-tanks",
      updater: "production",
      dir: "/opt/sloppy-tanks",
      url: VPS_MULTIPLAYER_URL,
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

// --provision installs the container runtime, Caddy, the updater, the systemd units and
// environment files first. With --dev it installs only the runtime, the updater, the dev
// unit, its environment, Caddy's config and the 8443 firewall rule. `--runtime podman`
// or `--runtime docker` switches the provisioned servers to that runtime; without it
// they keep the one they use (deploy/vps/install-runtime.sh).
const provision = process.argv.includes("--provision");
const runtimeIndex = process.argv.indexOf("--runtime");
const runtime = runtimeIndex === -1 ? "" : process.argv[runtimeIndex + 1];
if (runtimeIndex !== -1 && !(provision && ["podman", "docker"].includes(runtime))) {
  console.error("--runtime takes podman or docker, together with --provision");
  process.exit(1);
}
if (provision) {
  console.log(`Provisioning the ${target.name} server on ${VPS_SSH}`);
  ssh("rm -rf /root/sloppy-tanks-provision && mkdir -p /root/sloppy-tanks-provision");
  const runtimes = [
    "install-runtime.sh",
    "install-podman.sh",
    "install-docker.sh",
    "daemon.json",
    "sloppy-tanks-update",
    "sloppy-tanks-update.service",
    "sloppy-tanks-update.timer",
  ];
  const devFiles = [
    "sloppy-tanks-dev.container",
    "sloppy-tanks-dev.service",
    "sloppy-tanks-dev.env",
  ];
  const files = dev
    ? ["provision-dev.sh", ...runtimes, "Caddyfile", ...devFiles]
    : [
        "provision.sh",
        "install-caddy.sh",
        ...runtimes,
        "Caddyfile",
        "sloppy-tanks.container",
        "sloppy-tanks.service",
        "sloppy-tanks.env",
        ...devFiles,
      ];
  run("scp", [
    ...SSH_OPTIONS,
    ...files.map((file) => `deploy/vps/${file}`),
    `${VPS_SSH}:/root/sloppy-tanks-provision/`,
  ]);
  ssh(`bash /root/sloppy-tanks-provision/${files[0]} ${runtime}`);
}

const version = await contentVersion();
const build = await serverBuild();
console.log(
  `Deploying the ${target.name} multiplayer server (content ${version}, build ${build}) to ${VPS_SSH}`,
);

/** Stream `docker save` here into `podman load` or `docker load` on the VPS. */
function loadImageOnServer(image, runtime) {
  return new Promise((resolve, reject) => {
    const save = spawn("docker", ["save", image], { stdio: ["ignore", "pipe", "inherit"] });
    const load = spawn("ssh", [...SSH_OPTIONS, "-C", VPS_SSH, `${runtime} load`], {
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
    load.on("exit", done(`${runtime} load over SSH`));
  });
}

// Through Caddy, as players reach it. SLOPPY_SERVER_URL checks another server, such as a
// test VM.
const health = new URL(
  "/health",
  (process.env.SLOPPY_SERVER_URL ?? target.url).replace(/^ws/, "http"),
);
const readHealth = () =>
  fetch(health, { cache: "no-store" })
    .then((response) => response.json())
    .catch(() => ({}));
// The build, not only the content version, so a server-only change cannot pass against
// the previous process before a restart completes.
const isCurrent = (status) => status.contentVersion === version && status.serverBuild === build;

// Which unit runs the service: Podman's Quadlet unit, Docker's or the former binary one.
const unit = spawnSync(
  "ssh",
  [
    ...SSH_OPTIONS,
    VPS_SSH,
    `if [ -f /etc/containers/systemd/${target.service}.container ]; then echo podman; ` +
      `elif grep -qs 'docker run' /etc/systemd/system/${target.service}.service; then echo docker; ` +
      `else echo binary; fi; test -f /var/lib/sloppy-tanks/${target.updater}.image && echo pinned`,
  ],
  { encoding: "utf8" },
);
if (unit.status !== 0 && !unit.stdout) process.exit(unit.status ?? 1);
const [runtimeOnServer, pinned] = unit.stdout.trim().split("\n");

// Provisioning restarts a pinned service on its image, carried over when the runtime
// changed. When that already is this build, a deploy would only restart it again and
// leave a hold.
if (provision && runtimeOnServer !== "binary" && pinned && isCurrent(await readHealth())) {
  console.log(`The ${target.name} server already runs build ${build} under ${runtimeOnServer}`);
  process.exit(0);
}

if (runtimeOnServer !== "binary") {
  const image = `${SERVER_IMAGE}:${build}`;
  run("node", ["scripts/build-server-image.mjs", "--registry", SERVER_IMAGE]);
  await loadImageOnServer(image, runtimeOnServer);
  // Restarts the service and checks /health locally, rolling back if it fails.
  ssh(`sloppy-tanks-update ${target.updater} local ${image}`);
} else {
  // The former binary unit: a static musl binary, the host needs no runtime.
  run("node", ["scripts/build-server.mjs", "--vps"]);
  run("scp", [
    ...SSH_OPTIONS,
    "target/x86_64-unknown-linux-musl/server/sloppy-server",
    `${VPS_SSH}:${target.dir}/sloppy-server.new`,
  ]);
  // Rename in place so a crash-restart never runs a partially copied binary. The restart
  // resets live rooms; clients receive room-reset from the graceful shutdown.
  ssh(
    `cd ${target.dir} && chmod 755 sloppy-server.new && mv -f sloppy-server.new sloppy-server && systemctl restart ${target.service}`,
  );
}

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
console.log(`The ${target.name} multiplayer server is live at ${health.origin}`);
