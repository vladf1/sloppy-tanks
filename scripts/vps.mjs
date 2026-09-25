import { spawnSync } from "node:child_process";
import { VPS_SSH } from "./vps-host.mjs";

/** Read-only views of the VPS game server over SSH: `logs`, `stats` or `status`. */
const COMMANDS = {
  logs: "journalctl -u sloppy-tanks -f -n 50 --output cat",
  stats: "curl -s http://127.0.0.1:8787/stats",
  status: "systemctl status sloppy-tanks caddy --no-pager",
};
const name = process.argv[2];
if (!(name in COMMANDS)) {
  console.error(`Usage: node scripts/vps.mjs ${Object.keys(COMMANDS).join("|")}`);
  process.exit(1);
}
const result = spawnSync(
  "ssh",
  ["-o", "ConnectTimeout=15", ...(name === "logs" ? ["-t"] : []), VPS_SSH, COMMANDS[name]],
  { stdio: name === "stats" ? ["inherit", "pipe", "inherit"] : "inherit", encoding: "utf8" },
);
if (name === "stats" && result.status === 0)
  console.log(JSON.stringify(JSON.parse(result.stdout), null, 2));
process.exit(result.status ?? 1);
