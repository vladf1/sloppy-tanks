import { isIP } from "node:net";

/**
 * The multiplayer game servers: the machines of production and of the dev site, set up by
 * a provider startup script (deploy/server/cloud-init/) or deploy/server/ over SSH
 * (crates/server/README.md). The server directory lists the ones that run, each at the
 * nip.io name of its address (`1-2-3-4.nip.io` resolves to 1.2.3.4), which is all the
 * scripts need: they reach a machine over SSH as root at that address and check its server
 * through that name.
 */
/** The server's container image in the GitHub Container Registry. CI pushes it and moves
 * :production, `bun scripts/server.mjs publish --dev` pushes a checkout's build and moves
 * :dev, and the machines pull it (deploy/server/sloppy-tanks-update names it too). */
export const SERVER_IMAGE = "ghcr.io/vladf1/sloppy-tanks-server";

/** The server directory (directory/worker.ts): each machine pushes its rooms there, and the
 * pages of its site read every server's rooms from there, each under its environment, the
 * role (production or dev). */
export const DIRECTORY_URL = "https://sloppy-tanks-directory.fridman.me";

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

/** The environment of production's machines, or with `dev` the dev site's. */
export const environment = (dev) => (dev ? "dev" : "production");

/** The machines the directory lists for production, or with `dev` the dev site's; with
 * `ip`, that one machine, listed or not (one whose server is down, or still booting).
 * Listing none is an error unless `noneListed` allows it. */
export async function gameServers(dev, ip, noneListed = false) {
  if (ip) return [serverMachine(environment(dev), ip)];
  const list = `${DIRECTORY_URL}/servers?environment=${environment(dev)}`;
  const response = await fetch(list, { signal: AbortSignal.timeout(10_000) });
  if (!response.ok) throw new Error(`${list} answered ${response.status}`);
  const { servers } = await response.json();
  if (!servers.length && !noneListed) {
    throw new Error(`${list} lists no running server; name a machine with --ip`);
  }
  return servers.map(({ url }) =>
    serverMachine(
      environment(dev),
      url.match(/^wss:\/\/([\d-]+)\.nip\.io$/)[1].replaceAll("-", "."),
    ),
  );
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
    /** The WebSocket address players reach the server by. */
    url: `wss://${site}`,
    /** Where the scripts check the server through Caddy. */
    checkUrl: `https://${site}`,
  };
}
