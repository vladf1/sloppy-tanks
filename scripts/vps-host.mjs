import { spawnSync } from "node:child_process";

/** The self-hosted multiplayer VPS (Vultr, Ubuntu); deployment files live in deploy/vps/. */
export const VPS_SSH = process.env.SLOPPY_VPS_SSH ?? "root@sloppy-tanks-server.fridman.me";
/**
 * accept-new trusts a fresh VPS's host key on first contact, so a replacement needs no
 * manual login, but still refuses a known host whose key has changed.
 */
export const VPS_SSH_OPTIONS = [
  "-o",
  "ConnectTimeout=15",
  "-o",
  "StrictHostKeyChecking=accept-new",
];
/** The server's container image in the GitHub Container Registry. CI pushes it and moves
 * :production; the VPS pulls it (deploy/vps/sloppy-tanks-update names it too). */
export const SERVER_IMAGE = "ghcr.io/vladf1/sloppy-tanks-server";
/** Public WebSocket URL of the production VPS server. */
export const VPS_MULTIPLAYER_URL = "wss://sloppy-tanks-server.fridman.me";
/**
 * The dev site's server: a second process on the same VPS behind Caddy on port 8443 of
 * the same hostname and certificate, so dev deploys never replace production's server.
 */
export const VPS_DEV_MULTIPLAYER_URL = "wss://sloppy-tanks-server.fridman.me:8443";

/**
 * Where the deploy scripts read a VPS server's /health: through Caddy, as players reach
 * it, unless SLOPPY_VPS_SSH names another machine, such as a replacement VPS by IP before
 * the A record moves to it. The public name still reaches the old server then, and the
 * new one has no certificate yet, so its server is read on its loopback listener over
 * SSH. SLOPPY_SERVER_URL reads any other server. `read()` resolves to the JSON body, or
 * to `{ error }` when the server does not answer.
 */
export function vpsHealth(dev) {
  const publicUrl = dev ? VPS_DEV_MULTIPLAYER_URL : VPS_MULTIPLAYER_URL;
  const sshHost = VPS_SSH.slice(VPS_SSH.lastIndexOf("@") + 1);
  if (process.env.SLOPPY_SERVER_URL || sshHost === new URL(publicUrl).hostname) {
    const health = new URL(
      "/health",
      (process.env.SLOPPY_SERVER_URL ?? publicUrl).replace(/^ws/, "http"),
    );
    return {
      where: health.href,
      read: () =>
        fetch(health, { cache: "no-store" })
          .then((response) => response.json())
          .catch((error) => ({ error: error.message })),
    };
  }
  // The loopback ports in deploy/vps/sloppy-tanks{,-dev}.env.
  const health = `http://127.0.0.1:${dev ? 8788 : 8787}/health`;
  return {
    where: `${health} on ${VPS_SSH}`,
    read: async () => {
      const result = spawnSync(
        "ssh",
        ["-o", "BatchMode=yes", ...VPS_SSH_OPTIONS, VPS_SSH, `curl -fsS --max-time 5 ${health}`],
        { encoding: "utf8" },
      );
      try {
        return JSON.parse(result.stdout);
      } catch {
        return { error: result.stderr.trim() || `ssh exited with ${result.status}` };
      }
    },
  };
}
