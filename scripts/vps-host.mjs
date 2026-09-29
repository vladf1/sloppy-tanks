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
/** Public WebSocket URL of the production VPS server. */
export const VPS_MULTIPLAYER_URL = "wss://sloppy-tanks-server.fridman.me";
/**
 * The dev site's server: a second process on the same VPS behind Caddy on port 8443 of
 * the same hostname and certificate, so dev deploys never replace production's server.
 */
export const VPS_DEV_MULTIPLAYER_URL = "wss://sloppy-tanks-server.fridman.me:8443";
