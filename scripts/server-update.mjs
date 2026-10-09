import { spawnSync } from "node:child_process";
import { SERVER_IMAGE, VPS_SSH, VPS_SSH_OPTIONS, vpsHealth } from "./vps-host.mjs";

/** Operate the VPS server's image through deploy/vps/sloppy-tanks-update over SSH.
 *
 *   pull [--dev] [--image TAG]   pull an image CI published (default :production) and switch
 *   rollback [--dev]             switch back to the previous image
 *   auto on|off|resume           enable or disable production auto-update, or lift a hold
 *
 * The updater restarts the service and checks /health, rolling back if the new image
 * does not come up. `--image` takes a registry tag such as `pr-12` or a full image name. */
const SSH_OPTIONS = ["-o", "BatchMode=yes", ...VPS_SSH_OPTIONS];
const [command, argument] = process.argv.slice(2);
const dev = process.argv.includes("--dev");
const service = dev ? "dev" : "production";

function ssh(script) {
  const result = spawnSync("ssh", [...SSH_OPTIONS, VPS_SSH, script], { stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

function imageOption() {
  const index = process.argv.indexOf("--image");
  if (index === -1) return "";
  const value = process.argv[index + 1];
  if (!/^[\w.:/@-]+$/.test(value ?? "")) throw new Error(`Not an image or tag: ${value}`);
  return value.includes(":") || value.includes("/") ? value : `${SERVER_IMAGE}:${value}`;
}

switch (command) {
  case "pull":
    ssh(`sloppy-tanks-update ${service} pull ${imageOption()}`);
    break;
  case "rollback":
    ssh(`sloppy-tanks-update ${service} rollback`);
    break;
  case "auto":
    if (argument === "on") ssh("systemctl enable --now sloppy-tanks-update.timer");
    else if (argument === "off") ssh("systemctl disable --now sloppy-tanks-update.timer");
    else if (argument === "resume") ssh("sloppy-tanks-update production resume");
    else {
      console.error("Usage: pnpm run server:auto-update on|off|resume");
      process.exit(1);
    }
    ssh("sloppy-tanks-update production status");
    process.exit(0);
    break;
  default:
    console.error("Usage: node scripts/server-update.mjs pull|rollback|auto [...]");
    process.exit(1);
}

// What players reach through Caddy, or the machine itself before the public name reaches it.
const health = vpsHealth(dev);
console.log(`${health.where}: ${JSON.stringify(await health.read())}`);
