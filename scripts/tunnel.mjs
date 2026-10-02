// A temporary public HTTPS link to a fresh production build, for trying the game on a
// phone or tablet (docs/cloudflare-tunnel.md). Builds `dist/`, serves only that under
// the default /sloppy-tanks/ base, and opens a Cloudflare quick tunnel to it; Ctrl-C
// stops both. The link changes on every run and works while this machine is awake.
// Restart after source changes: the tunnel serves the build, not the dev server.
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const port = Number(process.env.PORT) || 4179;
const TUNNEL_HOST = /https:\/\/[\w-]+\.trycloudflare\.com/;

if (spawnSync("cloudflared", ["--version"], { stdio: "ignore" }).status !== 0) {
  console.error("cloudflared is missing: brew install cloudflared");
  process.exit(1);
}
if (!process.argv.includes("--no-build")) {
  const build = spawnSync("pnpm", ["run", "build"], { stdio: "inherit" });
  if (build.status !== 0) process.exit(build.status ?? 1);
}

// Serve a directory holding only a link to dist/, so the page keeps its base path and
// nothing else in the checkout is reachable.
const root = mkdtempSync(join(tmpdir(), "sloppy-tunnel-"));
symlinkSync(resolve("dist"), join(root, "sloppy-tanks"));
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", root],
  { stdio: ["ignore", "ignore", "inherit"] },
);
const tunnel = spawn(
  "cloudflared",
  ["tunnel", "--url", `http://127.0.0.1:${port}`, "--no-autoupdate"],
  { stdio: ["ignore", "ignore", "pipe"] },
);
let announced = false;
tunnel.stderr.setEncoding("utf8").on("data", (text) => {
  const host = !announced && text.match(TUNNEL_HOST)?.[0];
  if (host) {
    announced = true;
    console.log(`\nOn your phone: ${host}/sloppy-tanks/\n(Ctrl-C stops the link.)`);
  }
});

const stop = (code = 0) => {
  server.kill();
  tunnel.kill();
  rmSync(root, { recursive: true, force: true });
  process.exit(code);
};
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => stop());
for (const child of [server, tunnel]) {
  child.on("exit", (code) => {
    console.error(`${child === server ? "The static server" : "cloudflared"} stopped (${code}).`);
    stop(code ?? 1);
  });
}
