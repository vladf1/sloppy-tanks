import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { devHeaders, devPages, devSiteHtml } from "./dev-site.ts";
import { healthBody } from "./page-health.ts";
import { lazyEntries, startupHtml } from "./startup-html.ts";
import { gameServer } from "./servers.mjs";

export const root = path.resolve(import.meta.dir, "..");
export const defaultBase = "/sloppy-tanks/";
const naming = {
  entry: "assets/[name]-[hash].[ext]",
  chunk: "assets/[name]-[hash].[ext]",
  asset: "assets/[name]-[hash].[ext]",
};

export interface SiteOptions {
  base?: string;
  development?: boolean;
  devSite?: boolean;
  outdir?: string;
  pages?: string[];
  entries?: string[];
  minify?: boolean;
}
export interface SiteBuild {
  files: Map<string, Blob>;
  metadata: Bun.BuildMetafile;
  entries: Map<string, string>;
  base: string;
}

export function validateBase(base: string): string {
  if (!/^\/(?:[\w-]+\/)*$/.test(base))
    throw new Error("Base must be an absolute directory path ending in /");
  return base;
}

/** CSS imports wait for their own stylesheet. Bun otherwise folds dynamic CSS into the
 * parent entry, downloading the touch UI and room styles before either is selected. */
function browserAssets(
  cssAssets: Map<string, { name: string; body: Blob }>,
  files: Map<string, Blob>,
  base: string,
): Bun.BunPlugin {
  return {
    name: "browser-assets",
    setup(build) {
      build.onResolve({ filter: /\.wasm\?url$/ }, (args) => ({
        path: path.resolve(args.resolveDir, args.path.slice(0, -4)),
      }));
      build.onLoad({ filter: /\.css$/ }, async (args) => {
        const asset = cssAssets.get(args.path);
        if (!asset) throw new Error(`Missing stylesheet: ${args.path}`);
        const { name, body } = asset;
        files.set(name, body);
        return {
          loader: "js",
          contents: `await new Promise((resolve,reject)=>{const link=document.createElement("link");link.rel="stylesheet";link.href=${JSON.stringify(base + name)};link.onload=resolve;link.onerror=()=>reject(new Error("Could not load "+link.href));document.head.append(link)});`,
        };
      });
    },
  };
}

/** Build only the requested pages; local fixture pages use this on their first request. */
export async function buildSite(options: SiteOptions = {}): Promise<SiteBuild> {
  const base = validateBase(
    options.base ?? (options.devSite ? "/" : process.env.DEPLOY_BASE || defaultBase),
  );
  const development = options.development ?? process.env.NODE_ENV === "development";
  const minify = options.minify ?? !development;
  const multiplayerUrl = options.devSite
    ? gameServer(true).url
    : (process.env.MULTIPLAYER_URL ?? "");
  const define: Record<string, string> = {
    "import.meta.env.BASE_URL": JSON.stringify(base),
    "import.meta.env.DEV": String(development),
    "import.meta.env.MULTIPLAYER_URL": JSON.stringify(multiplayerUrl),
    "process.env.NODE_ENV": JSON.stringify(development ? "development" : "production"),
  };
  const pages = options.pages ?? [
    "index.html",
    ...(options.devSite ? ["test-pages.html", ...devPages.map((page) => page.path)] : []),
  ];
  const main = pages.includes("index.html");
  const entrypoints = [
    ...(main ? lazyEntries.map((entry) => entry.source) : []),
    ...(options.entries ?? []),
  ];
  const files = new Map<string, Blob>();
  // Only the game and room client start texture-bake workers. Keep labs builds
  // independent of both generated gameplay engines.
  if (entrypoints.some((entry) => entry === "src/game.ts" || entry === "src/net/client.ts")) {
    for (const [source, key] of [
      ["src/game/texture-bake-worker.ts", "WEBGPU_BAKE_WORKER_URL"],
      ["src/game/texture-bake-worker-webgl.ts", "WEBGL_BAKE_WORKER_URL"],
    ]) {
      const worker = await Bun.build({
        entrypoints: [path.join(root, source)],
        root,
        target: "browser",
        minify,
        naming,
        define,
      });
      const output = worker.outputs.find((file) => file.kind === "entry-point");
      if (!output) throw new Error(`Missing worker output: ${source}`);
      const name = output.path.replace(/^\.\//, "");
      files.set(name, output);
      define[`import.meta.env.${key}`] = JSON.stringify(base + name);
    }
  }

  // Compile styles before entering bundler callbacks: Bun.build is not reentrant.
  const cssAssets = new Map<string, { name: string; body: Blob }>();
  for (const source of ["src/touch-controls.css", "src/net/multiplayer.css", "tools/lab.css"]) {
    const result = await Bun.build({
      entrypoints: [path.join(root, source)],
      root,
      target: "browser",
      minify,
    });
    const css = result.outputs.find((file) => file.path.endsWith(".css"));
    if (!css) throw new Error(`Missing stylesheet: ${source}`);
    const name = `assets/${path.basename(source, ".css")}-${css.hash}.css`;
    cssAssets.set(path.join(root, source), { name, body: css });
  }
  const virtual = new Map<string, string>();
  const templates = new Map<string, string>();
  for (const page of pages) {
    let html = await readFile(path.join(root, page), "utf8");
    // Public URLs retain their names and follow the deployment base.
    html = html.replace(/(\b(?:src|href)=["'])\/(?!\/)/g, `$1${base}`);
    html = devSiteHtml(html, base);
    let inline = 0;
    html = html.replace(
      /<script\b([^>]*)>([\s\S]*?)<\/script>/gi,
      (tag, attrs: string, contents: string) => {
        if (!/\btype=["']module["']/.test(attrs)) return tag;
        const src = /\bsrc=["']([^"']+)["']/.exec(attrs)?.[1];
        const entry = src
          ? path.posix.normalize(path.posix.join(path.posix.dirname(page), src))
          : `${page}.inline-${inline++}.ts`;
        if (!src) virtual.set(path.join(root, entry), contents);
        entrypoints.push(entry);
        return `<!-- module:${entry} -->`;
      },
    );
    // Lab styles are authored beside the page, outside public/.
    html = html.replace(/<link\b[^>]*rel=["']stylesheet["'][^>]*>/g, (tag) => {
      const href = /href=["']([^"']+)["']/.exec(tag)?.[1];
      if (!href?.startsWith(".")) return tag;
      const source = path.posix.normalize(path.posix.join(path.posix.dirname(page), href));
      const asset = cssAssets.get(path.join(root, source));
      if (!asset) throw new Error(`Unknown page stylesheet: ${source}`);
      files.set(asset.name, asset.body);
      return `<link rel="stylesheet" href="${base}${asset.name}">`;
    });
    templates.set(page, html);
  }
  const metadata: Bun.BuildMetafile = { inputs: {}, outputs: {} };
  const entries = new Map<string, string>();
  if (entrypoints.length) {
    const result = await Bun.build({
      entrypoints: [...new Set(entrypoints)].map((entry) => path.join(root, entry)),
      root,
      target: "browser",
      format: "esm",
      splitting: true,
      minify,
      naming,
      publicPath: base,
      define,
      metafile: true,
      loader: { ".wasm": "file" },
      plugins: [
        {
          name: "inline-modules",
          setup(build) {
            build.onResolve({ filter: /\.html\.inline-\d+\.ts$/ }, (args) => ({
              path: args.path,
              namespace: "inline-html",
            }));
            build.onLoad({ filter: /.*/, namespace: "inline-html" }, (args) => {
              const contents = virtual.get(args.path);
              if (contents === undefined) throw new Error(`Unknown inline module ${args.path}`);
              return { contents, loader: "ts", resolveDir: path.dirname(args.path) };
            });
          },
        },
        browserAssets(cssAssets, files, base),
      ],
    });
    if (!result.metafile) throw new Error("Missing browser build metadata");
    Object.assign(metadata.inputs, result.metafile.inputs);
    for (const [name, output] of Object.entries(result.metafile.outputs)) {
      const relative = name.replace(/^\.\//, "");
      metadata.outputs[relative] = output;
      if (output.entryPoint)
        entries.set(
          output.entryPoint.replace(/^inline-html:/, "").replace(root + path.sep, ""),
          relative,
        );
    }
    for (const output of result.outputs) files.set(output.path.replace(/^\.\//, ""), output);
  }
  for (const [page, template] of templates) {
    let html = template.replace(/<!-- module:(.+?) -->/g, (_match, entry: string) => {
      const file = entries.get(entry);
      if (!file) throw new Error(`No output for ${entry}`);
      return `<script type="module" src="${base}${file}"></script>`;
    });
    if (page === "index.html") {
      const startup = await Bun.build({
        entrypoints: [path.join(root, "src/main.ts")],
        root,
        target: "browser",
        minify: true,
        define,
        plugins: [
          {
            name: "lazy-startup",
            setup(build) {
              build.onResolve({ filter: /^\./ }, (args) => {
                const target = path.resolve(args.resolveDir, args.path).replace(/\.ts$/, "");
                const lazy = lazyEntries.find(
                  (entry) => path.join(root, entry.source).slice(0, -3) === target,
                );
                return lazy ? { path: lazy.id, external: true } : undefined;
              });
            },
          },
        ],
      });
      const js = startup.outputs.find((file) => file.path.endsWith(".js"));
      const css = startup.outputs.find((file) => file.path.endsWith(".css"));
      if (!js || !css) throw new Error("Missing inline startup code or styles");
      const imports = new Map(
        lazyEntries.map((entry) => [entry.id, base + entries.get(entry.source)]),
      );
      const script = (await js.text()).replace(
        /import\((["'`])(sloppy:\w+)\1\)/g,
        (_load, _quote, id: string) => `import(${JSON.stringify(imports.get(id))})`,
      );
      const graph = new Set<string>();
      const visit = (file: string) => {
        const name = file.replace(/^\.\//, "").replace(new RegExp(`^${base}`), "");
        if (graph.has(name)) return;
        graph.add(name);
        for (const dependency of metadata.outputs[name]?.imports ?? []) {
          if (dependency.kind === "import-statement") visit(dependency.path);
        }
      };
      visit(entries.get("src/game.ts")!);
      const binary = (name: string) => {
        const matches = [...files.keys()].filter(
          (file) => file.startsWith(`assets/${name}-`) && file.endsWith(".wasm"),
        );
        if (matches.length !== 1)
          throw new Error(`Expected one ${name} binary, found ${matches.length}`);
        return base + matches[0];
      };
      html = await startupHtml(
        html,
        base,
        multiplayerUrl,
        script,
        await css.text(),
        [...graph].map((file) => base + file),
        [binary("engine_bg"), binary("engine-webgl_bg")],
      );
    }
    files.set(page, new Blob([html], { type: "text/html; charset=utf-8" }));
  }
  if (main)
    files.set(
      "health/index.html",
      new Blob([await healthBody(new Date().toISOString())], { type: "text/html; charset=utf-8" }),
    );
  if (options.devSite) files.set("_headers", new Blob([devHeaders]));
  if (options.outdir) {
    const directory = path.resolve(options.outdir);
    if (!directory.startsWith(root + path.sep) || directory === root)
      throw new Error("Build output must be a directory inside the checkout");
    await rm(directory, { recursive: true, force: true });
    await cp(path.join(root, "public"), directory, { recursive: true });
    for (const [name, body] of files) {
      const filename = path.join(directory, name);
      await mkdir(path.dirname(filename), { recursive: true });
      await writeFile(filename, Buffer.from(await body.arrayBuffer()));
    }
  }
  return { files, metadata, entries, base };
}
