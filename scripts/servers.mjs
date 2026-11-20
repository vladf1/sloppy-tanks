import { isIP } from "node:net";

/**
 * The multiplayer game servers: the machines of production and of the dev site, set up by
 * a provider startup script (deploy/server/cloud-init/) or deploy/server/ over SSH
 * (crates/server/README.md). The server directory lists the ones that run, each at the
 * nip.io name of its address (`1-2-3-4.nip.io` resolves to 1.2.3.4), which is all the
 * scripts need: they reach a machine over SSH as root at that address and check its server
 * through that name.
 */
/** The server's container image in the GitHub Container Registry. CI pushes it and adds
 * it to production's deployment file, `bun scripts/server.mjs publish --dev` pushes a
 * checkout's build and adds it to dev's (scripts/promote-server.mjs, scripts/deployment.mjs),
 * and the machines pull it (deploy/server/sloppy-tanks-update names it too). */
export const SERVER_IMAGE = "ghcr.io/vladf1/sloppy-tanks-server";

/** How many content versions an environment keeps running: an App Store version keeps its
 * server while newer ones reach TestFlight and App Review, and the iOS and Mac apps may be
 * released apart. The updater runs no more than this. */
export const KEPT_VERSIONS = 5;

/** The deployment's versions with `added` first, in place of another image of its content
 * version, and the oldest beyond KEPT_VERSIONS dropped. */
export function addVersion(versions, added) {
  return [
    added,
    ...versions.filter(({ contentVersion }) => contentVersion !== added.contentVersion),
  ].slice(0, KEPT_VERSIONS);
}

/** The label that listed an environment's versions on its image tag before the deployment
 * file did. Machines created before the deployment file still read it, so the promote
 * script writes it too until each has been created again; then it goes. */
export const VERSIONS_LABEL = "me.fridman.sloppy-tanks.versions";

/** The versions VERSIONS_LABEL lists, newest first: `<content>@<digest>` separated by
 * spaces. */
export function parseVersions(text) {
  return text
    .split(" ")
    .filter(Boolean)
    .map((entry) => {
      const match = entry.match(/^([0-9a-f]{8,64})@(sha256:[0-9a-f]{64})$/);
      if (!match) throw new Error(`Not a listed version: ${entry}`);
      return { contentVersion: match[1], digest: match[2] };
    });
}

export function formatVersions(versions) {
  return versions.map(({ contentVersion, digest }) => `${contentVersion}@${digest}`).join(" ");
}

/** The server directory (directory/worker.ts): each machine pushes its rooms there, and the
 * pages of its site read every server's rooms from there, each under its environment, the
 * role (production or dev). */
export const DIRECTORY_URL = "https://directory.sloppytanks.com";

/**
 * accept-new trusts a fresh machine's host key on first contact, so a new one needs no
 * manual login, but still refuses a known address whose key has changed. Every ssh and
 * scp a command runs shares one connection per machine, kept for a minute, so a key that
 * asks before each use (1Password's SSH agent) asks once per command, not per connection.
 */
export const SSH_OPTIONS = [
  "-o",
  "BatchMode=yes",
  "-o",
  "ConnectTimeout=15",
  "-o",
  "StrictHostKeyChecking=accept-new",
  "-o",
  "ControlMaster=auto",
  "-o",
  "ControlPath=~/.ssh/sloppy-tanks-%C",
  "-o",
  "ControlPersist=60",
];

/** The environment a command line names: the dev site's with `--dev`, the App Store apps'
 * with `--app-store`, else production's. */
export function environment(args) {
  if (args.includes("--dev")) return "dev";
  return args.includes("--app-store") ? "app-store" : "production";
}

/** The value a command line gives `name`, as `--name VALUE` or `--name=VALUE`, or
 * nothing when it leaves the flag out. A flag without a value (the last argument, or one
 * followed by another flag) throws rather than reading as left out: `--ip` with nothing
 * after it would otherwise reach every machine the directory lists. */
export function option(args, name) {
  const inline = args.find((arg) => arg.startsWith(`${name}=`));
  if (inline !== undefined) {
    const value = inline.slice(name.length + 1);
    if (value === "") throw new Error(`${name} needs a value`);
    return value;
  }
  const index = args.indexOf(name);
  if (index === -1) return undefined;
  const value = args[index + 1];
  if (value === undefined || value.startsWith("--")) throw new Error(`${name} needs a value`);
  return value;
}

/** The machines the directory lists for `role`, an environment, each once however many
 * versions it runs; with `ip`, that one machine, listed or not (one whose server is down,
 * or still booting). Listing none is an error unless `noneListed` allows it. */
export async function gameServers(role, ip, noneListed = false) {
  if (ip) return [serverMachine(role, ip)];
  const list = `${DIRECTORY_URL}/servers?environment=${role}`;
  const response = await fetch(list, { signal: AbortSignal.timeout(10_000) });
  if (!response.ok) throw new Error(`${list} answered ${response.status}`);
  const { servers } = await response.json();
  if (!servers.length && !noneListed) {
    throw new Error(`${list} lists no running server; name a machine with --ip`);
  }
  return [...new Set(servers.map(({ url }) => machineAddress(url)))].map((address) =>
    serverMachine(role, address),
  );
}

/** The IPv4 address of the machine a listed server runs on: one entry per content version
 * it runs (`wss://1-2-3-4.nip.io/v/<content version>`), or one at its root. */
export function machineAddress(url) {
  const match = url.match(/^wss:\/\/(\d+-\d+-\d+-\d+)\.nip\.io(?:\/v\/[0-9a-f]+)?$/);
  if (!match) throw new Error(`Not a game server address: ${url}`);
  return match[1].replaceAll("-", ".");
}

/** How the scripts reach the machine at `ip` and what it serves. */
export function serverMachine(role, ip) {
  if (isIP(ip) !== 4) throw new Error(`Not an IPv4 address: ${ip}`);
  const site = `${ip.replaceAll(".", "-")}.nip.io`;
  return {
    role,
    /** The machine's public IPv4 address. */
    ip,
    /** SSH destination. */
    ssh: `root@${ip}`,
    /** The name Caddy serves and obtains a certificate for. */
    site,
    /** The machine's root, where Caddy serves its newest version (players reach each
     * version under `/v/<content version>`). */
    url: `wss://${site}`,
    /** Where the scripts check the newest version through Caddy. */
    checkUrl: `https://${site}`,
  };
}
