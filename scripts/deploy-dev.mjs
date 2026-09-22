import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

// Fixed destination: never infer a Pages project or branch from the checkout.
const info = JSON.parse(
  readFileSync(new URL("../dist-dev/build-info.json", import.meta.url), "utf8"),
);
console.log(
  `Publishing dev build ${info.builtAt} (${info.commit}${info.dirty ? ", local changes" : ""})`,
);
const result = spawnSync(
  "wrangler",
  [
    "pages",
    "deploy",
    "dist-dev",
    "--project-name",
    "sloppy-tanks-dev",
    "--branch",
    "main",
    "--commit-dirty=true",
  ],
  {
    cwd: new URL("..", import.meta.url),
    stdio: "inherit",
    env: { ...process.env, CLOUDFLARE_ACCOUNT_ID: "b49a59dfb5edf913223ad13eeab8d740" },
  },
);
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
