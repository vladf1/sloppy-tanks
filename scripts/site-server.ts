import { watch } from "node:fs";
import { stat } from "node:fs/promises";
import path from "node:path";
import { buildSite, defaultBase, root, validateBase, type SiteBuild } from "./site-build.ts";
import { healthBody } from "./page-health.ts";

interface ServerOptions {
  hostname?: string;
  port?: number;
  base?: string;
}

/** Decode before checking containment; neither source files nor sibling directories are public. */
export function requestPath(request: Request, base: string): string | Response {
  const url = new URL(request.url);
  if (base !== "/" && url.pathname === base.slice(0, -1)) {
    url.pathname = base;
    return Response.redirect(url, 302);
  }
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

async function staticFile(
  directory: string,
  relative: string,
  request: Request,
): Promise<Response> {
  const filename = path.join(directory, relative);
  const info = await stat(filename).catch(() => undefined);
  if (info?.isDirectory()) {
    const url = new URL(request.url);
    if (!url.pathname.endsWith("/")) {
      url.pathname += "/";
      return Response.redirect(url, 302);
    }
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

export function startDevServer({
  hostname = "127.0.0.1",
  port = Number(process.env.PORT) || 5173,
  base = process.env.DEPLOY_BASE || defaultBase,
}: ServerOptions = {}) {
  validateBase(base);
  let revision = 0;
  let outputs = new Map<string, Blob>();
  let previousOutputs = new Map<string, Blob>();
  const builds = new Map<string, Promise<SiteBuild>>();
  const listeners = new Set<ReadableStreamDefaultController>();
  const started = new Date().toISOString();
  const debugEntries = ["src/game/ui.ts", "src/game/engine-api.ts"];
  const build = (page: string, entry?: string) => {
    const key = entry ?? page;
    let pending = builds.get(key);
    if (!pending) {
      const generation = revision;
      pending = buildSite({
        base,
        development: true,
        pages: entry ? [] : [page],
        entries: entry ? [entry] : page === "index.html" ? debugEntries : [],
      })
        .then((result) => {
          if (generation === revision)
            for (const [name, body] of result.files) outputs.set(name, body);
          return result;
        })
        .catch((error) => {
          if (builds.get(key) === pending) builds.delete(key);
          throw error;
        });
      builds.set(key, pending);
    }
    return pending;
  };
  let timer: ReturnType<typeof setTimeout> | undefined;
  const changed = () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      revision++;
      builds.clear();
      previousOutputs = outputs;
      outputs = new Map();
      for (const listener of listeners) listener.enqueue(`data: ${revision}\n\n`);
    }, 25);
  };
  const server = Bun.serve({
    hostname,
    port,
    reusePort: false,
    development: false,
    async fetch(request, server) {
      const relative = requestPath(request, base);
      if (relative instanceof Response) return relative;
      if (relative === "__reload") {
        // An idle reload stream stays open until its page leaves.
        server.timeout(request, 0);
        let controller: ReadableStreamDefaultController;
        return new Response(
          new ReadableStream({
            start(value) {
              controller = value;
              listeners.add(value);
              value.enqueue(`data: ${revision}\n\n`);
            },
            cancel() {
              listeners.delete(controller);
            },
          }),
          { headers: { "Content-Type": "text/event-stream", "Cache-Control": "no-cache" } },
        );
      }
      if (relative === "health" || relative === "health/")
        return new Response(await healthBody(started), {
          headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" },
        });
      const page = relative || "index.html";
      if (
        (page === "index.html" ||
          page === "test-pages.html" ||
          /^(tests|tools)\/[\w.-]+\.html$/.test(page)) &&
        (await Bun.file(path.join(root, page)).exists())
      ) {
        let generation: number;
        let result: SiteBuild;
        do {
          generation = revision;
          result = await build(page);
        } while (generation !== revision);
        const body = result.files.get(page);
        if (!body) throw new Error(`No page output for ${page}`);
        const reload = `<script>{const updates=new EventSource(${JSON.stringify(base + "__reload")});updates.onmessage=event=>{if(event.data!==${JSON.stringify(String(revision))})location.reload()};}</script>`;
        return new Response(
          (await body.text()).replace("</body>", () => `${reload}</body>`),
          { headers: { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" } },
        );
      }
      // Browser checks import these entries from the same bundle as the running game.
      if (debugEntries.includes(relative) || relative === "tests/engine-fixture.ts") {
        const result = debugEntries.includes(relative)
          ? await build("index.html")
          : await build("", relative);
        return new Response(
          `export * from ${JSON.stringify(base + result.entries.get(relative))};`,
          { headers: { "Content-Type": "text/javascript" } },
        );
      }
      const output = outputs.get(relative) ?? previousOutputs.get(relative);
      if (output) return new Response(output);
      return staticFile(path.join(root, "public"), relative, request);
    },
    error(error) {
      console.error(error);
      return new Response(String(error), { status: 500 });
    },
  });
  // Register watchers only after binding succeeds, including occupied-port retries.
  const watchers = ["src", "tests", "tools", "scripts", "public"].map((directory) =>
    watch(path.join(root, directory), { recursive: true }, (_event, filename) => {
      if (!filename?.includes("node_modules")) changed();
    }),
  );
  const rootFiles = new Set(["index.html", "test-pages.html", "tsconfig.json", "package.json"]);
  watchers.push(
    watch(root, (_event, filename) => {
      if (filename && rootFiles.has(filename.toString())) changed();
    }),
  );
  return {
    url: new URL(base, server.url),
    stop() {
      clearTimeout(timer);
      watchers.forEach((watcher) => watcher.close());
      listeners.forEach((listener) => listener.close());
      listeners.clear();
      return server.stop(true);
    },
  };
}
