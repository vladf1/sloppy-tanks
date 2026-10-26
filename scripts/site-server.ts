import { watch } from "node:fs";
import { readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { defaultBase, root, validateBase } from "./site-build.ts";
import { healthBody } from "./page-health.ts";

interface ServerOptions {
  hostname?: string;
  port?: number;
  base?: string;
}

/** A redirect by path alone, which keeps the scheme and host the browser used: behind the
 * tunnel's HTTPS, an absolute URL from the request would point at plain HTTP. */
function redirect(location: string): Response {
  return new Response(null, { status: 302, headers: { Location: location } });
}

/** Decode before checking containment; neither source files nor sibling directories are public.
 * The site root and the bare base both lead to the page, query and all. */
export function requestPath(request: Request, base: string): string | Response {
  const url = new URL(request.url);
  if (base !== "/" && (url.pathname === "/" || url.pathname === base.slice(0, -1)))
    return redirect(base + url.search);
  let pathname: string;
  try {
    pathname = decodeURIComponent(url.pathname);
  } catch {
    return new Response("Invalid URL", { status: 400 });
  }
  if (!pathname.startsWith(base)) return new Response("Not found", { status: 404 });
  const relative = pathname.slice(base.length);
  if (
    relative.split("/").some((part) => part === ".." || part.startsWith(".")) ||
    relative.includes("\\") ||
    relative.includes("\0")
  )
    return new Response("Not found", { status: 404 });
  return relative;
}

export async function staticFile(
  directory: string,
  relative: string,
  request: Request,
): Promise<Response> {
  const filename = path.join(directory, relative);
  const info = await stat(filename).catch(() => undefined);
  if (info?.isDirectory()) {
    const url = new URL(request.url);
    if (!url.pathname.endsWith("/")) return redirect(`${url.pathname}/${url.search}`);
    return staticFile(directory, relative + "index.html", request);
  }
  return info?.isFile()
    ? new Response(Bun.file(filename))
    : new Response("Not found", { status: 404 });
}

export function startStaticServer({
  directory = "dist",
  hostname = "127.0.0.1",
  port = 4173,
  base = defaultBase,
}: ServerOptions & { directory?: string } = {}) {
  validateBase(base);
  return Bun.serve({
    hostname,
    port,
    reusePort: false,
    development: false,
    async fetch(request) {
      const relative = requestPath(request, base);
      return relative instanceof Response
        ? relative
        : staticFile(path.resolve(directory), relative, request);
    },
  });
}

/** Bun's dev server bundles no workers, so the texture-bake workers are built when a page
 * asks for them, at these paths under the base. */
const BAKE_WORKERS = {
  PUBLIC_WEBGPU_BAKE_WORKER_URL: ["bake-workers/webgpu.js", "src/game/texture-bake-worker.ts"],
  PUBLIC_WEBGL_BAKE_WORKER_URL: ["bake-workers/webgl.js", "src/game/texture-bake-worker-webgl.ts"],
} as const;

async function bakeWorker(source: string): Promise<Response> {
  const { outputs } = await Bun.build({
    entrypoints: [path.join(root, source)],
    target: "browser",
  });
  return new Response(outputs[0], {
    headers: { "Content-Type": "text/javascript; charset=utf-8", "Cache-Control": "no-store" },
  });
}

/** Bun's own dev server, with hot reloading: it bundles every page itself, through the
 * plugin in bunfig.toml (`dev-pages-plugin.ts`), and the build values the pages read. */
export async function startDevServer({
  hostname = "127.0.0.1",
  port = Number(process.env.PORT) || 5173,
  base = process.env.DEPLOY_BASE || defaultBase,
}: ServerOptions = {}) {
  validateBase(base);
  // The build values the pages read; dev-pages-plugin.ts writes them into the modules.
  Object.assign(process.env, {
    PUBLIC_BASE_URL: base,
    PUBLIC_DEV: "true",
    PUBLIC_MULTIPLAYER_URL: process.env.MULTIPLAYER_URL ?? "",
    ...Object.fromEntries(Object.entries(BAKE_WORKERS).map(([key, [url]]) => [key, base + url])),
  });
  const routes: Record<string, Bun.HTMLBundle | (() => Promise<Response>)> = {};
  const pages = new Bun.Glob("{tests,tools}/*.html").scan({ cwd: root });
  for (const page of ["index.html", "test-pages.html", ...(await Array.fromAsync(pages))]) {
    const bundle = ((await import(path.join(root, page))) as { default: Bun.HTMLBundle }).default;
    routes[page === "index.html" ? base : base + page] = bundle;
  }
  const started = new Date().toISOString();
  const health = async () =>
    new Response(await healthBody(started), {
      headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" },
    });
  routes[base + "health"] = routes[base + "health/"] = health;
  for (const [url, source] of Object.values(BAKE_WORKERS))
    routes[base + url] = () => bakeWorker(source);
  const server = Bun.serve({
    hostname,
    port,
    reusePort: false,
    // Page console output reaches the terminal too, beside the server's own errors.
    development: { hmr: true, console: true },
    routes,
    async fetch(request) {
      const relative = requestPath(request, base);
      if (relative instanceof Response) return relative;
      // Engine binaries keep their source paths (`dev-pages-plugin.ts`).
      if (/^src\/generated\/[\w-]+\/[\w-]+\.wasm$/.test(relative))
        return staticFile(root, relative, request);
      return staticFile(path.join(root, "public"), relative, request);
    },
  });
  // Bun watches the files it bundles, not Battle Setup's markup, which the plugin reads
  // into the game page; rewriting that page unchanged makes Bun bundle it again.
  const game = path.join(root, "index.html");
  const setup = watch(path.join(root, "src/game/battle-setup.html"), () => {
    void readFile(game).then((html) => writeFile(game, html));
  });
  return {
    url: new URL(base, server.url),
    stop() {
      setup.close();
      return server.stop(true);
    },
  };
}
