import { readFileSync } from "node:fs";
import { isIP } from "node:net";

/**
 * The multiplayer game servers: one machine each for production and the dev site, listed
 * in deploy/servers.json and set up by deploy/server/ (crates/server/README.md). Each entry
 * has the machine's public `ip`, which every script reaches it by over SSH as root, and
 * the `hostnames` Caddy serves; nothing else differs between the machines.
 *
 * `{dashed-ip}` in a hostname stands for the address with dashes: `{dashed-ip}.nip.io`
 * becomes `1-2-3-4.nip.io`, which nip.io resolves to 1.2.3.4 (sslip.io and others work the
 * same way). Such a name needs no DNS record of ours, so a new machine has a certificate
 * and a working address as soon as it is provisioned; any other hostname needs its own A
 * record pointing at `ip`. Players use the first hostname, and the scripts check through
 * the first one made from the address, which reaches this machine whatever the other
 * names' DNS says.
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

const DASHED_IP = "{dashed-ip}";

/** The production server, or with `dev` the dev site's. */
export function gameServer(dev) {
  const role = dev ? "dev" : "production";
  return serverMachine(role, list[role]);
}

/** How the scripts reach a machine from its deploy/servers.json entry. */
export function serverMachine(role, { ip, hostnames }) {
  if (!hostnames?.length) throw new Error(`${role} in deploy/servers.json lists no hostnames`);
  if (ip && isIP(ip) !== 4) throw new Error(`${role}'s ip is not an IPv4 address: ${ip}`);
  const fromIp = (name) => name.includes(DASHED_IP);
  const expand = (name) => name.replaceAll(DASHED_IP, ip.replaceAll(".", "-"));
  // Until the machine exists, neither do the names made from its address.
  const sites = ip ? hostnames.map(expand) : hostnames.filter((name) => !fromIp(name));
  for (const name of sites) {
    if (/[{}]/.test(name)) throw new Error(`Only ${DASHED_IP} can stand in a hostname: ${name}`);
  }
  if (!sites.length) throw new Error(`${role} in deploy/servers.json needs an ip`);
  const machineName = ip ? hostnames.filter(fromIp).map(expand)[0] : undefined;
  return {
    role,
    /** The machine's public IPv4 address, or null until it exists. */
    ip,
    /** SSH destination; throws until the machine exists. */
    get ssh() {
      if (!ip) throw new Error(`${role} in deploy/servers.json has no ip yet`);
      return `root@${ip}`;
    },
    /** Every name Caddy serves, which it obtains certificates for. */
    sites,
    /** The hostnames that need their own DNS record pointing at `ip`. */
    dnsNames: hostnames.filter((name) => !fromIp(name)),
    /** The WebSocket address players' pages use. */
    url: `wss://${sites[0]}`,
    /** Where the scripts check the server through Caddy: the name that reaches this machine. */
    checkUrl: `https://${machineName ?? sites[0]}`,
  };
}
