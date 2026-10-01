import { spawnSync } from "node:child_process";
import { VPS_SSH, VPS_SSH_OPTIONS } from "./vps-host.mjs";

/** Read-only views of a VPS game server over SSH: `logs`, `stats` or `status`.
 * `--dev` reads the dev site's server (sloppy-tanks-dev on loopback port 8788). */
const dev = process.argv.includes("--dev");
const service = dev ? "sloppy-tanks-dev" : "sloppy-tanks";
const COMMANDS = {
  logs: `journalctl -u ${service} -f -n 50 --output cat`,
  stats: `curl -s http://127.0.0.1:${dev ? 8788 : 8787}/stats`,
  // The image state and, for production, auto-update's latest checks.
  status:
    `systemctl status ${service} caddy --no-pager; ` +
    `sloppy-tanks-update ${dev ? "dev" : "production"} status` +
    (dev ? "" : " && journalctl -u sloppy-tanks-update -n 5 --no-pager --output cat"),
};
const name = process.argv[2];
if (!(name in COMMANDS)) {
  console.error(`Usage: node scripts/vps.mjs ${Object.keys(COMMANDS).join("|")}`);
  process.exit(1);
}
const result = spawnSync(
  "ssh",
  [...VPS_SSH_OPTIONS, ...(name === "logs" ? ["-t"] : []), VPS_SSH, COMMANDS[name]],
  { stdio: name === "stats" ? ["inherit", "pipe", "inherit"] : "inherit", encoding: "utf8" },
);
if (name === "stats" && result.status === 0)
  console.log(JSON.stringify(JSON.parse(result.stdout), null, 2));
process.exit(result.status ?? 1);
