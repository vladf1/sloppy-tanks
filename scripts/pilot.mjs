// Pilots from this machine: `pnpm run pilot -- --room CODE --hunt NAME` builds the
// `sloppy-pilot` command line (crates/pilot) in release and runs it with the arguments
// after `--` (`pnpm run pilot -- --help` lists them). Like the browser engine and the
// server it is stamped with the content version (`SLOPPY_CONTENT_VERSION`): a server
// running other game code refuses its pilots, so run it from the commit the server runs.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { contentVersion } from "./content-version.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
// `pnpm run pilot -- …` passes its `--` through.
const args = process.argv.slice(2);
if (args[0] === "--") args.shift();
const result = spawnSync(
  "cargo",
  [
    "run",
    "--locked",
    "--release",
    "--quiet",
    "-p",
    "sloppy-pilot",
    "--bin",
    "sloppy-pilot",
    "--",
    ...args,
  ],
  {
    cwd: root,
    stdio: "inherit",
    env: { ...process.env, SLOPPY_CONTENT_VERSION: await contentVersion() },
  },
);
if (result.error) console.error(`cargo: ${result.error.message}. See README.md for Rust setup.`);
process.exit(result.status ?? 1);
