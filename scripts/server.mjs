import { spawn, spawnSync } from "node:child_process";
import { contentVersion, serverBuild } from "./content-version.mjs";
import { environment, gameServers, option, SERVER_IMAGE, SSH_OPTIONS } from "./servers.mjs";

/** Operate the game server machines, one after another and over SSH except for
 * `publish`: the ones the server directory lists for production, or with `--dev` for the
 * dev site, with `--app-store` for the App Store apps', or with `--ip ADDRESS` that
 * machine, listed or not. See crates/server/README.md.
 *
 *   publish --dev            build the image here and add it to :dev, whose versions the dev
 *                            machines run; wait until each serves it
 *   update [--image TAG]     follow the tag now (start, switch and stop versions without
 *                            waiting for players), or run another image CI published and hold
 *   deploy [--force]         build the image here, copy it over SSH and run it (the SSH
 *                            fallback; for production only from a clean origin/main)
 *   rollback                 switch the newest version back to its previous image
 *   auto-update on|off|resume
 *   status | logs | stats
 *
 * deploy/server/sloppy-tanks-update on the machine does every change: it runs a server per
 * content version the tag lists, checks each one's /health and restores the previous image
 * when a new one fails. Anything but following the tag leaves a hold that auto-update
 * respects (on dev, until the next publish). */
const repo = new URL("..", import.meta.url);
const HEALTH_TIMEOUT_MS = 120_000;
const [command, argument] = process.argv.slice(2);
const dev = process.argv.includes("--dev");
const role = environment(process.argv);
const USAGE =
  "Usage: bun scripts/server.mjs publish|update|deploy|rollback|auto-update|status|logs|stats [--dev|--app-store] [--ip ADDRESS]";
/** Read before anything is pushed, listed or reached over SSH: a flag without its value
 * stops the command rather than widening it to every machine. */
const ip = commandOption("--ip");
const imageArgument = commandOption("--image");
/** What a publish pushed, for every machine to serve. It pushes before looking for
 * machines: one still booting is not listed yet, and starts it by itself. */
const published = command === "publish" ? await publish() : undefined;
const servers = await gameServers(role, ip, published !== undefined);
if (published && servers.length === 0) {
  console.log(`The directory lists no ${role} server; one that boots starts this by itself.`);
}
/** The machine the current command runs on. */
let server;

function run(program, args, options = {}) {
  const result = spawnSync(program, args, { cwd: repo, stdio: "inherit", ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
  return result;
}
const ssh = (script, options) => run("ssh", [...SSH_OPTIONS, server.ssh, script], options);
const git = (...args) => spawnSync("git", args, { cwd: repo, encoding: "utf8" });

function commandOption(name) {
  try {
    return option(process.argv, name);
  } catch (error) {
    console.error(`${error.message}\n${USAGE}`);
    process.exit(1);
  }
}

/** The server's /health through Caddy, by the name that always reaches this machine.
 * Always an object: a restarting machine can answer `null`, which the waits treat as
 * not ready yet. */
let health;
const readHealth = () =>
  fetch(health, { cache: "no-store", signal: AbortSignal.timeout(10_000) })
    .then((response) => response.json())
    .then((body) =>
      body !== null && typeof body === "object"
        ? body
        : { error: `answered ${JSON.stringify(body)}` },
    )
    .catch((error) => ({ error: error.cause?.message ?? error.message }));

/** Waits until /health passes `expected`, through Caddy as players reach it. A new
 * machine's certificate arrives within a minute of Caddy's first start. */
async function waitForHealth(expected, describe, timeout = HEALTH_TIMEOUT_MS) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const status = await readHealth();
    if (expected(status)) return status;
    if (Date.now() > deadline)
      throw new Error(`${health} reports ${JSON.stringify(status)}; expected ${describe}`);
    await new Promise((resolve) => setTimeout(resolve, 2000));
  }
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
  // App Store machines run the versions the Apps workflow uploaded to TestFlight, which a
  // checkout's build would replace with another.
  if (role === "app-store" && !process.argv.includes("--force")) {
    console.error(
      "App Store machines run the servers of the apps' builds: run the Apps workflow (or --force).",
    );
    process.exit(1);
  }
  // Production runs what main published; a local build of anything else would stop
  // matching the site's content version and send every visitor a reload message.
  if (role === "production" && !process.argv.includes("--force")) {
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
  run(process.execPath, ["scripts/build-server-image.mjs", "--registry", SERVER_IMAGE]);
  await loadImageOnServer(image);
  // Restarts that version's server, ending its live rooms (clients get room-reset from the
  // graceful shutdown), or starts one, and rolls back if the new image does not report
  // healthy. The site's root then serves it.
  ssh(`sloppy-tanks-update local ${image}`);
  // The build, not only the content version, so a server-only change cannot pass against
  // the previous process.
  await waitForHealth(
    (status) => status.contentVersion === version && status.serverBuild === build,
    `content ${version}, build ${build}`,
  );
  console.log(`The ${server.role} server is live at ${server.url}`);
}

/** Builds the server image from this checkout and pushes it as dev-<server build> (unless
 * the registry already holds an image of that server build), then adds it to the versions
 * :dev lists (scripts/promote-server.mjs). The dev machines check :dev every 10 seconds and
 * start it by themselves, so publishing needs registry access but no SSH. Returns what
 * their /health reports once they serve it at their root. */
async function publish() {
  if (!dev) {
    console.error("publish is for the dev servers (--dev); production runs CI's images.");
    process.exit(1);
  }
  const version = await contentVersion();
  const build = await serverBuild();
  console.log(`Publishing the dev server (content ${version}, build ${build}) to :dev`);
  const result = spawnSync(
    process.execPath,
    ["scripts/build-server-image.mjs", "--registry", SERVER_IMAGE, "--push", "--dev"],
    { cwd: repo, stdio: "inherit" },
  );
  // CI's image of the same server build when there is one (build-server-image.mjs then
  // pushes nothing), else the one just pushed.
  const source =
    result.status === 0
      ? [`${SERVER_IMAGE}:${build}`, `${SERVER_IMAGE}:dev-${build}`].find(inRegistry)
      : undefined;
  if (result.status === 0 && !source) {
    console.error(`The registry holds no image of server build ${build} after the push.`);
    process.exit(1);
  }
  const promoted = source
    ? spawnSync(process.execPath, ["scripts/promote-server.mjs", "dev", source], {
        cwd: repo,
        stdio: "inherit",
      })
    : result;
  if (promoted.status !== 0) {
    console.error(
      "Publishing needs Docker logged in to ghcr.io with a token that can write packages:\n" +
        "  gh auth refresh -h github.com -s write:packages\n" +
        '  gh auth token | docker login ghcr.io --username "$(gh api user -q .login)" --password-stdin',
    );
    process.exit(promoted.status ?? 1);
  }
  return { version, build };
}

/** Whether the registry holds `image`. */
function inRegistry(image) {
  return (
    spawnSync("docker", ["buildx", "imagetools", "inspect", image], { cwd: repo }).status === 0
  );
}

/** A registry tag such as `pr-12`, or a full image name. */
function imageOption() {
  if (imageArgument === undefined) return "";
  if (!/^[\w.:/@-]+$/.test(imageArgument)) throw new Error(`Not an image or tag: ${imageArgument}`);
  return imageArgument.includes(":") || imageArgument.includes("/")
    ? imageArgument
    : `${SERVER_IMAGE}:${imageArgument}`;
}

async function printHealth() {
  console.log(`${health}: ${JSON.stringify(await readHealth())}`);
}

for (server of servers) {
  health = new URL("/health", server.checkUrl);
  await runCommand();
}

async function runCommand() {
  switch (command) {
    case "publish": {
      const { version, build } = published;
      console.log(`Waiting for ${health.origin}, which checks :dev every 10 seconds`);
      // The newest version listed is the one the site's root serves.
      await waitForHealth(
        (status) => status.contentVersion === version && status.serverBuild === build,
        `content ${version}, build ${build} (bun run server:status --dev shows why not)`,
      );
      console.log(`The dev server is live at ${server.url}`);
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
      // A machine created again starts with auto-update on (bootstrap.sh).
      if (argument === "on") ssh("systemctl enable --now sloppy-tanks-update.timer");
      else if (argument === "off") ssh("systemctl disable --now sloppy-tanks-update.timer");
      else if (argument === "resume") ssh("sloppy-tanks-update resume");
      else {
        console.error("Usage: bun run server:auto-update on|off|resume [--dev|--app-store]");
        process.exit(1);
      }
      ssh("sloppy-tanks-update status");
      break;
    case "status":
      // Each version's server and image state, and auto-update's latest checks.
      ssh(
        "systemctl status caddy --no-pager; sloppy-tanks-update status && " +
          "journalctl -u sloppy-tanks-update -n 5 --no-pager --output cat",
      );
      break;
    case "logs":
      run("ssh", [
        ...SSH_OPTIONS,
        "-t",
        server.ssh,
        // Every version's server, and the updater starting and stopping them.
        "journalctl -u 'sloppy-tanks-*' -f -n 50 --output cat",
      ]);
      break;
    case "stats":
      // Each version's /stats on its loopback port.
      ssh("sloppy-tanks-update stats");
      break;
    default:
      console.error(USAGE);
      process.exit(1);
  }
}
