// A temporary public HTTPS link to a fresh production build, for trying the game on a
// phone or tablet (docs/cloudflare-tunnel.md). Builds `dist/`, serves only that under
// the default /sloppy-tanks/ base, and opens a Cloudflare quick tunnel to it; Ctrl-C
// stops both. The link changes on every run and works while this machine is awake.
// Restart after source changes: the tunnel serves the build, not the dev server.
//
// Opened locally (the `tunnel` launch entry's preview), the root page shows the
// public link; visitors through the tunnel go straight to the game.
import { spawn, spawnSync } from "node:child_process";
import { createReadStream, statSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, relative, resolve } from "node:path";

const port = Number(process.env.PORT) || 4179;
const BASE = "/sloppy-tanks/";
const TUNNEL_HOST = /https:\/\/[\w-]+\.trycloudflare\.com/;
const LINK_PAGE_REFRESH_SECONDS = 2;
const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".css": "text/css",
  ".wasm": "application/wasm",
  ".webp": "image/webp",
  ".svg": "image/svg+xml",
  ".mp3": "audio/mpeg",
  ".json": "application/json",
  ".txt": "text/plain; charset=utf-8",
};

if (spawnSync("cloudflared", ["--version"], { stdio: "ignore" }).status !== 0) {
  console.error("cloudflared is missing: brew install cloudflared");
  process.exit(1);
}
if (!process.argv.includes("--no-build")) {
  const build = spawnSync("pnpm", ["run", "build"], { stdio: "inherit" });
  if (build.status !== 0) process.exit(build.status ?? 1);
}

const dist = resolve("dist");
let link = "";

/** The local root page: the phone link once the tunnel has one. */
function linkPage() {
  const body = link
    ? `<p>Open on your phone:</p><p><a href="${link}">${link}</a></p>
       <button onclick="navigator.clipboard.writeText('${link}').then(()=>this.textContent='Copied')">Copy link</button>
       <p class="note">The link changes on every run and stops with the tunnel. Restart after source changes.</p>`
    : `<p>Waiting for Cloudflare to assign a link…</p>`;
  return `<!doctype html><html lang="en"><head><meta charset="utf-8">
    <meta name="viewport" content="width=device-width,initial-scale=1">
    ${link ? "" : `<meta http-equiv="refresh" content="${LINK_PAGE_REFRESH_SECONDS}">`}
    <title>Sloppy Tanks tunnel</title><style>
    body{margin:0;min-height:100vh;display:grid;place-content:center;gap:4px;padding:16px;
      font:16px system-ui,sans-serif;background:#0d1b2a;color:#e8f2fa;text-align:center}
    a{color:#ffcf38;font-size:20px;font-weight:700;word-break:break-all}
    button{justify-self:center;padding:8px 18px;border:0;border-radius:6px;background:#ffcf38;
      color:#153954;font-weight:800;cursor:pointer}
    .note{color:#9fbcd2;font-size:13px}</style></head>
    <body><h1>Sloppy Tanks tunnel</h1>${body}</body></html>`;
}

/** Serve only files inside dist/, under the page's base path. */
function serveBuild(request, response) {
  const path = decodeURIComponent(new URL(request.url, "http://local").pathname);
  if (path === BASE.slice(0, -1)) {
    response.writeHead(301, { location: BASE }).end();
    return;
  }
  let file = resolve(dist, `.${path.slice(BASE.length - 1)}`);
  if (relative(dist, file).startsWith("..")) {
    response.writeHead(403).end();
    return;
  }
  try {
    if (statSync(file).isDirectory()) file = join(file, "index.html");
    const size = statSync(file).size;
    response.writeHead(200, {
      "content-type": TYPES[extname(file)] ?? "application/octet-stream",
      "content-length": size,
    });
    createReadStream(file).pipe(response);
  } catch {
    response.writeHead(404).end("Not found");
  }
}

const server = createServer((request, response) => {
  const path = new URL(request.url, "http://local").pathname;
  if (path.startsWith(BASE) || path === BASE.slice(0, -1)) {
    serveBuild(request, response);
  } else if (path === "/" && /^(localhost|127\.0\.0\.1)(:|$)/.test(request.headers.host ?? "")) {
    response.writeHead(200, { "content-type": TYPES[".html"], "cache-control": "no-store" });
    response.end(linkPage());
  } else if (path === "/") {
    response.writeHead(302, { location: BASE }).end();
  } else {
    response.writeHead(404).end("Not found");
  }
});
server.listen(port, "127.0.0.1", () => {
  console.log(`Link page: http://localhost:${port}/`);
});

const tunnel = spawn(
  "cloudflared",
  ["tunnel", "--url", `http://127.0.0.1:${port}`, "--no-autoupdate"],
  { stdio: ["ignore", "ignore", "pipe"] },
);
tunnel.stderr.setEncoding("utf8").on("data", (text) => {
  const host = !link && text.match(TUNNEL_HOST)?.[0];
  if (host) {
    link = `${host}${BASE}`;
    console.log(`\nOn your phone: ${link}\n(Ctrl-C stops the link.)`);
  }
});

const stop = (code = 0) => {
  tunnel.kill();
  server.close();
  process.exit(code);
};
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => stop());
server.on("error", (error) => {
  console.error(`The static server stopped: ${error.message}`);
  stop(1);
});
tunnel.on("exit", (code) => {
  console.error(`cloudflared stopped (${code}).`);
  stop(code ?? 1);
});
