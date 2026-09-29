import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { contentVersion, serverBuild } from "./content-version.mjs";

/** Build the native multiplayer server. `--vps` cross-compiles the static Linux x86_64
 * (musl) binary the VPS runs; otherwise it builds for this machine. The server stamps
 * the same content version as the browser build, and a fingerprint of its own inputs,
 * which `/health` reports so deploys and the redeploy check notice changes. */
const repo = fileURLToPath(new URL("..", import.meta.url));
export const VPS_TARGET = "x86_64-unknown-linux-musl";
const vps = process.argv.includes("--vps");
const args = ["build", "--locked", "--profile", "server", "-p", "sloppy-server"];
if (vps) args.push("--target", VPS_TARGET);
const result = spawnSync("cargo", args, {
  cwd: repo,
  stdio: "inherit",
  env: {
    ...process.env,
    SLOPPY_CONTENT_VERSION: await contentVersion(),
    SLOPPY_SERVER_BUILD: await serverBuild(),
  },
});
if (result.error) console.error(`cargo: ${result.error.message}. See README.md for Rust setup.`);
if (result.status !== 0) process.exit(result.status ?? 1);
console.log(`Built ${vps ? `target/${VPS_TARGET}/server` : "target/server"}/sloppy-server`);
