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
/** Public WebSocket URL of the VPS server; the dev site build points its client here. */
export const VPS_MULTIPLAYER_URL = "wss://sloppy-tanks-server.fridman.me";
