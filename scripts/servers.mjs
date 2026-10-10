import { readFileSync } from "node:fs";
import { BlockList, isIP } from "node:net";

/**
 * The multiplayer game servers: one machine each for production and the dev site, listed
 * in deploy/servers.json and set up by deploy/server/ (crates/server/README.md). Each entry
 * has the machine's public `ip`, which every script reaches it by over SSH as root, and
 * an optional `hostname` for players; nothing else differs between the machines.
 *
 * Caddy on the machine serves the hostname and the machine's nip.io name (`1-2-3-4.nip.io`
 * resolves to 1.2.3.4), so a new machine has a certificate and a working address as soon as
 * it is provisioned; a hostname only needs its DNS record to point at `ip`. Players use the
 * hostname when there is one, and the scripts check through the nip.io name, which always
 * reaches this machine whatever the hostname's DNS says.
 *
 * SLOPPY_SERVERS names another list, such as one of local test machines.
 */
const listFile = process.env.SLOPPY_SERVERS ?? new URL("../deploy/servers.json", import.meta.url);
const list = JSON.parse(readFileSync(listFile, "utf8"));

/** The server's container image in the GitHub Container Registry. CI pushes it and moves
 * :production; the machines pull it (deploy/server/sloppy-tanks-update names it too). */
export const SERVER_IMAGE = "ghcr.io/vladf1/sloppy-tanks-server";

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

// Addresses outside the internet, such as a local test machine's, get no nip.io name:
// no certificate authority can reach them to issue one.
const privateAddresses = new BlockList();
for (const [network, prefix] of [
  ["10.0.0.0", 8],
  ["100.64.0.0", 10],
  ["127.0.0.0", 8],
  ["172.16.0.0", 12],
  ["192.168.0.0", 16],
]) {
  privateAddresses.addSubnet(network, prefix);
}

/** The nip.io name that resolves to an IPv4 address: 45.63.56.58 → 45-63-56-58.nip.io. */
export function nipName(ip) {
  if (isIP(ip) !== 4) throw new Error(`Not an IPv4 address: ${ip}`);
  return `${ip.replaceAll(".", "-")}.nip.io`;
}

/** The production server, or with `dev` the dev site's. */
export function gameServer(dev) {
  const role = dev ? "dev" : "production";
  return serverMachine(role, list[role]);
}

/** How the scripts reach a machine from its deploy/servers.json entry. */
export function serverMachine(role, { ip, hostname }) {
  const nip = ip && !privateAddresses.check(ip) ? nipName(ip) : undefined;
  const sites = [hostname, nip].filter(Boolean);
  if (!sites.length) throw new Error(`${role} in deploy/servers.json needs an ip`);
  return {
    role,
    /** SSH destination; throws until the machine exists. */
    get ssh() {
      if (!ip) throw new Error(`${role} in deploy/servers.json has no ip yet`);
      return `root@${ip}`;
    },
    /** Every name Caddy serves, which it obtains certificates for. */
    sites,
    /** The WebSocket address players' pages use. */
    url: `wss://${sites[0]}`,
    /** Where the scripts check the server through Caddy: the name that reaches this machine. */
    checkUrl: `https://${nip ?? sites[0]}`,
  };
}
