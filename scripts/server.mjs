import { spawn, spawnSync } from "node:child_process";
import { contentVersion, serverBuild } from "./content-version.mjs";
import { environment, gameServers, SERVER_IMAGE, SSH_OPTIONS } from "./servers.mjs";

/** Operate the game server machines, one after another and over SSH except for
 * `publish` and `release-app`: the ones the server directory lists for production, or
 * with `--dev` for the dev site, with `--app-store` for the App Store apps', or with
 * `--ip ADDRESS` that machine, listed or not. See crates/server/README.md.
 *
 *   publish --dev            build the image here and push it as :dev, which the dev
 *                            machines follow; wait until each serves it
 *   release-app [--image TAG]
 *                            point :app-store at the App Store build TestFlight has
 *                            (:app-store-candidate, or TAG), which the App Store machines
 *                            follow; wait until each serves it (the candidate machine
 *                            keeps following :app-store-candidate)
 *   update [--image TAG]     pull an image CI published (default the followed tag) and switch
 *   deploy [--force]         build the image here, copy it over SSH and switch (the SSH
 *                            fallback; for production only from a clean origin/main)
 *   rollback                 switch back to the previous image
 *   auto-update on|off|resume
 *   status | logs | stats
 *
 * deploy/server/sloppy-tanks-update on the machine does every switch: it restarts the
 * server, checks /health there and restores the previous image if the new one fails.
 * Anything but following the tag leaves a hold that auto-update respects (on dev, until
 * the next publish). */
const repo = new URL("..", import.meta.url);
const HEALTH_TIMEOUT_MS = 120_000;
const RELEASE_TIMEOUT_MS = 12 * 60_000;
const [command, argument] = process.argv.slice(2);
const dev = process.argv.includes("--dev");
const role = command === "release-app" ? "app-store" : environment(process.argv);
/** What a publish or release pushed, for every machine to switch to. It pushes before
 * looking for machines: one still booting is not listed yet, and starts it by itself. */
const published =
  command === "publish" ? await publish() : command === "release-app" ? release() : undefined;
const servers = await gameServers(role, option("--ip"), published !== undefined);
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

function option(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
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
  // App Store machines run the released apps' content version, which a checkout's build
  // would replace with another.
  if (role === "app-store" && !process.argv.includes("--force")) {
    console.error(
      "App Store machines run the released apps' server: use release-app (or --force).",
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

/** Builds the server image from this checkout and pushes it as dev-<server build> and :dev
 * (or points :dev at an image of that server build the registry already holds). The dev
 * machines check :dev every 10 seconds and switch to it by themselves, so publishing needs
 * registry access but no SSH. Returns what their /health reports once they run it. */
async function publish() {
  if (!dev) {
    console.error("publish is for the dev servers (--dev); production runs CI's images.");
    process.exit(1);
  }
  const version = await contentVersion();
  const build = await serverBuild();
  console.log(`Publishing the dev server (content ${version}, build ${build}) as :dev`);
  const result = spawnSync(
    process.execPath,
    [
      "scripts/build-server-image.mjs",
      "--registry",
      SERVER_IMAGE,
      "--push",
      "--dev",
      "--tag",
      "dev",
    ],
    { cwd: repo, stdio: "inherit" },
  );
  if (result.status !== 0) {
    console.error(
      "Publishing needs Docker logged in to ghcr.io with a token that can write packages:\n" +
        "  gh auth refresh -h github.com -s write:packages\n" +
        '  gh auth token | docker login ghcr.io --username "$(gh api user -q .login)" --password-stdin',
    );
    process.exit(result.status ?? 1);
  }
  return { version, build };
}

/** The content version a server image in the registry plays, or nothing when the registry
 * has no such image. */
function imageContentVersion(image) {
  const inspected = spawnSync(
    "docker",
    ["buildx", "imagetools", "inspect", image, "--format", "{{json .Image}}"],
    { cwd: repo, encoding: "utf8" },
  );
  if (inspected.status !== 0) return undefined;
  const config = JSON.parse(inspected.stdout);
  // A manifest list maps each platform to its config; the image has only linux/amd64.
  const labels = (config.config ?? config["linux/amd64"]?.config)?.Labels ?? {};
  const version = labels["me.fridman.sloppy-tanks.content-version"];
  if (!version) {
    console.error(`${image} has no content version label; is it a server image?`);
    process.exit(1);
  }
  return version;
}

/** Points :app-store at the App Store build's server image, :app-store-candidate (which
 * the Apps workflow moves with each TestFlight upload) unless `--image` names another, and
 * returns the content version the App Store machines then report, beside the candidate's:
 * the candidate machine follows :app-store-candidate, not this release. Needs Docker
 * logged in to ghcr.io with a token that can write packages, as `publish` does. */
function release() {
  const candidateImage = `${SERVER_IMAGE}:app-store-candidate`;
  const source = imageOption() || candidateImage;
  const version = imageContentVersion(source);
  if (!version) {
    console.error(`The registry has no ${source}; run the Apps workflow on main first.`);
    process.exit(1);
  }
  const candidate = source === candidateImage ? version : imageContentVersion(candidateImage);
  console.log(`Releasing ${source} (content ${version}) to the App Store machines as :app-store`);
  run("docker", ["buildx", "imagetools", "create", "--tag", `${SERVER_IMAGE}:app-store`, source]);
  return { version, candidate };
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

for (server of servers) {
  health = new URL("/health", server.checkUrl);
  await runCommand();
}

async function runCommand() {
  switch (command) {
    case "publish": {
      const { version, build } = published;
      console.log(`Waiting for ${health.origin}, which checks :dev every 10 seconds`);
      await waitForHealth(
        (status) => status.contentVersion === version && status.serverBuild === build,
        `content ${version}, build ${build} (bun run server:status --dev shows why not)`,
      );
      console.log(`The dev server is live at ${server.url}`);
      break;
    }
    case "release-app": {
      // A machine switches once nobody plays on it, at most ten minutes after the tag moves
      // (sloppy-tanks-update), so this waits a little longer than that.
      // Nothing tells the candidate machine from the others but what it plays. When the
      // release is not the candidate (`--image`), a machine that plays the candidate's
      // version, now or once its own pending update lands, is taken for it: it never
      // switches to the release, so the wait ends there too.
      const { version, candidate } = published;
      console.log(`Waiting for ${health.origin}, which switches once its rooms are empty`);
      const status = await waitForHealth(
        ({ contentVersion }) =>
          contentVersion === version || (candidate !== undefined && contentVersion === candidate),
        `content ${version} (bun run server:status --app-store shows why not)`,
        RELEASE_TIMEOUT_MS,
      );
      if (status.contentVersion === version) {
        console.log(`The App Store server at ${server.url} plays the released apps' version`);
      } else {
        console.log(
          `${health.origin} plays the candidate's version (content ${candidate}): not waiting ` +
            "for it. If it follows :app-store, it switches once its rooms are empty.",
        );
      }
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
        "Usage: bun scripts/server.mjs publish|release-app|update|deploy|rollback|auto-update|status|logs|stats [--dev|--app-store] [--ip ADDRESS]",
      );
      process.exit(1);
  }
}
