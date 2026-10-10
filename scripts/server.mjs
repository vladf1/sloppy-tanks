import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { contentVersion, serverBuild } from "./content-version.mjs";
import { gameServer, SERVER_IMAGE, SSH_OPTIONS } from "./servers.mjs";

/** Operate a game server machine (deploy/servers.json) over SSH; `--dev` picks the dev
 * site's machine, otherwise production's. See crates/server/README.md.
 *
 *   provision                set the machine up (again), then start CI's :production image
 *                            if no server runs (deploy:dev then replaces it on dev)
 *   update [--image TAG]     pull an image CI published (default :production) and switch
 *   deploy [--force]         build the image here, copy it over SSH and switch (dev's
 *                            deploy; production's fallback, from a clean origin/main)
 *   rollback                 switch back to the previous image
 *   auto-update on|off|resume
 *   status | logs | stats
 *
 * deploy/server/sloppy-tanks-update on the machine does every switch: it restarts the
 * server, checks /health there and restores the previous image if the new one fails.
 * Anything but following :production leaves a hold that auto-update respects. */
const repo = new URL("..", import.meta.url);
const HEALTH_TIMEOUT_MS = 120_000;
const [command, argument] = process.argv.slice(2);
const dev = process.argv.includes("--dev");
const server = gameServer(dev);

function run(program, args, options = {}) {
  const result = spawnSync(program, args, { cwd: repo, stdio: "inherit", ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
  return result;
}
const ssh = (script, options) => run("ssh", [...SSH_OPTIONS, server.ssh, script], options);
const succeeds = (script) =>
  spawnSync("ssh", [...SSH_OPTIONS, server.ssh, script], { stdio: "ignore" }).status === 0;
const git = (...args) => spawnSync("git", args, { cwd: repo, encoding: "utf8" });

function option(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

/** The server's /health through Caddy, by the name that always reaches this machine. */
const health = new URL("/health", server.checkUrl);
const readHealth = () =>
  fetch(health, { cache: "no-store", signal: AbortSignal.timeout(10_000) })
    .then((response) => response.json())
    .catch((error) => ({ error: error.cause?.message ?? error.message }));

/** Waits until /health passes `expected`, through Caddy as players reach it. A new
 * machine's certificate arrives within a minute of Caddy's first start. */
async function waitForHealth(expected, describe) {
  const deadline = Date.now() + HEALTH_TIMEOUT_MS;
  for (;;) {
    const status = await readHealth();
    if (expected(status)) return status;
    if (Date.now() > deadline)
      throw new Error(`${health} reports ${JSON.stringify(status)}; expected ${describe}`);
    await new Promise((resolve) => setTimeout(resolve, 2000));
  }
}

/** Uploads deploy/server/ with this machine's names in the Caddyfile, and runs provision.sh. */
function provision() {
  console.log(
    `Provisioning the ${server.role} server on ${server.ssh} (${server.sites.join(", ")})`,
  );
  const staging = mkdtempSync(join(tmpdir(), "sloppy-tanks-provision-"));
  try {
    const files = [
      "provision.sh",
      "sloppy-tanks.container",
      "caddy.container",
      "sloppy-tanks-update",
      "sloppy-tanks-update.service",
      "sloppy-tanks-update.timer",
    ].map((name) => fileURLToPath(new URL(`../deploy/server/${name}`, import.meta.url)));
    const caddyfile = readFileSync(new URL("../deploy/server/Caddyfile", import.meta.url), "utf8");
    writeFileSync(
      join(staging, "Caddyfile"),
      caddyfile.replaceAll("{$SITES}", server.sites.join(", ")),
    );
    ssh("rm -rf /root/sloppy-tanks-provision && mkdir -p /root/sloppy-tanks-provision");
    run("scp", [
      ...SSH_OPTIONS,
      ...files,
      join(staging, "Caddyfile"),
      `${server.ssh}:/root/sloppy-tanks-provision/`,
    ]);
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
  ssh("bash /root/sloppy-tanks-provision/provision.sh");
}

/** Streams `docker save` here into `podman load` on the machine. */
function loadImageOnServer(image) {
  return new Promise((resolve, reject) => {
    const save = spawn("docker", ["save", image], { stdio: ["ignore", "pipe", "inherit"] });
    const load = spawn("ssh", [...SSH_OPTIONS, "-C", server.ssh, "podman load"], {
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

/** Builds the server image from this checkout and switches the machine to it. */
async function deploy() {
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
    if (git("rev-parse", "HEAD").stdout.trim() !== git("rev-parse", "origin/main").stdout.trim()) {
      console.error("Production deploys come from origin/main; check it out (or pass --force).");
      process.exit(1);
    }
  }
  const version = await contentVersion();
  const build = await serverBuild();
  console.log(
    `Deploying the ${server.role} server (content ${version}, build ${build}) to ${server.ssh}`,
  );
  const image = `${SERVER_IMAGE}:${build}`;
  run("node", ["scripts/build-server-image.mjs", "--registry", SERVER_IMAGE]);
  await loadImageOnServer(image);
  // Restarts the server, ending live rooms (clients get room-reset from the graceful
  // shutdown), and rolls back if the new image does not report healthy.
  ssh(`sloppy-tanks-update local ${image}`);
  // The build, not only the content version, so a server-only change cannot pass against
  // the previous process.
  await waitForHealth(
    (status) => status.contentVersion === version && status.serverBuild === build,
    `content ${version}, build ${build}`,
  );
  console.log(`The ${server.role} server is live at ${server.url}`);
}

/** A registry tag such as `pr-12`, or a full image name. */
function imageOption() {
  const value = option("--image");
  if (value === undefined) return "";
  if (!/^[\w.:/@-]+$/.test(value)) throw new Error(`Not an image or tag: ${value}`);
  return value.includes(":") || value.includes("/") ? value : `${SERVER_IMAGE}:${value}`;
}

async function printHealth() {
  console.log(`${health}: ${JSON.stringify(await readHealth())}`);
}

switch (command) {
  case "provision": {
    provision();
    // A fresh machine has no pinned image; one that has keeps running it.
    if (!succeeds("test -f /var/lib/sloppy-tanks/image")) ssh("sloppy-tanks-update pull");
    const status = await waitForHealth((status) => !!status.contentVersion, "a /health report");
    console.log(`The ${server.role} server answers at ${health.origin}: ${JSON.stringify(status)}`);
    break;
  }
  case "update":
    ssh(`sloppy-tanks-update pull ${imageOption()}`);
    await printHealth();
    break;
  case "deploy":
    await deploy();
    break;
  case "rollback":
    ssh("sloppy-tanks-update rollback");
    await printHealth();
    break;
  case "auto-update":
    if (argument === "on") ssh("systemctl enable --now sloppy-tanks-update.timer");
    else if (argument === "off") ssh("systemctl disable --now sloppy-tanks-update.timer");
    else if (argument === "resume") ssh("sloppy-tanks-update resume");
    else {
      console.error("Usage: pnpm run server:auto-update on|off|resume [--dev]");
      process.exit(1);
    }
    ssh("sloppy-tanks-update status");
    break;
  case "status":
    // The image state and auto-update's latest checks.
    ssh(
      "systemctl status sloppy-tanks caddy --no-pager; sloppy-tanks-update status && " +
        "journalctl -u sloppy-tanks-update -n 5 --no-pager --output cat",
    );
    break;
  case "logs":
    run("ssh", [
      ...SSH_OPTIONS,
      "-t",
      server.ssh,
      "journalctl -u sloppy-tanks -f -n 50 --output cat",
    ]);
    break;
  case "stats": {
    const result = ssh("curl -fsS http://127.0.0.1:8787/stats", {
      stdio: ["inherit", "pipe", "inherit"],
      encoding: "utf8",
    });
    console.log(JSON.stringify(JSON.parse(result.stdout), null, 2));
    break;
  }
  default:
    console.error(
      "Usage: node scripts/server.mjs provision|update|deploy|rollback|auto-update|status|logs|stats [--dev]",
    );
    process.exit(1);
}
