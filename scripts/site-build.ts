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

/** A metafile source path from the checkout, like `src/game.ts`. Bun writes them from the
 * working directory, whatever `root` says. */
export function sourcePath(file: string): string {
  return path.relative(root, path.resolve(file));
}

/** `metafile` with every source path from the checkout, so readers match the same keys
 * from any working directory. Output paths are already relative to the output root. */
export function sourceMetafile(metafile: Bun.BuildMetafile): Bun.BuildMetafile {
  const inputs: Bun.BuildMetafile["inputs"] = {};
  for (const [file, input] of Object.entries(metafile.inputs)) {
    inputs[sourcePath(file)] = {
      ...input,
      imports: input.imports.map((entry) =>
        entry.external ? entry : { ...entry, path: sourcePath(entry.path) },
      ),
    };
  }
  const outputs: Bun.BuildMetafile["outputs"] = {};
  for (const [file, output] of Object.entries(metafile.outputs)) {
    outputs[file] = {
      ...output,
      inputs: Object.fromEntries(
        Object.entries(output.inputs).map(([input, part]) => [sourcePath(input), part]),
      ),
      entryPoint: output.entryPoint && sourcePath(output.entryPoint),
    };
  }
  return { inputs, outputs };
}

/** The load of the stylesheet at `href`, one per page however many modules ask for it, so
 * the import that starts a lazy module can start its styles too (`withStyles`). */
function styleLoad(href: string): string {
  return `((globalThis.sloppyStyles??={})[${JSON.stringify(href)}]??=new Promise((resolve,reject)=>{const link=document.createElement("link");link.rel="stylesheet";link.href=${JSON.stringify(href)};link.onload=resolve;link.onerror=()=>reject(new Error("Could not load "+link.href));document.head.append(link)}))`;
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
        return { loader: "js", contents: `await ${styleLoad(base + name)};` };
      });
    },
  };
}

/** The production pages, or the dev site, or a check's copy of either: only the requested
 * pages and entries. The dev server lets Bun bundle pages itself (`site-server.ts`). */
export async function buildSite(options: SiteOptions = {}): Promise<SiteBuild> {
  const base = validateBase(
    options.base ?? (options.devSite ? "/" : process.env.DEPLOY_BASE || defaultBase),
  );
  const development = options.development ?? process.env.NODE_ENV === "development";
  const minify = options.minify ?? !development;
  const multiplayerUrl = options.devSite
    ? gameServer(true).url
    : (process.env.MULTIPLAYER_URL ?? "");
  // The page reads its build values from `process.env` (src/build-env.d.ts).
  const define: Record<string, string> = {
    "process.env.PUBLIC_BASE_URL": JSON.stringify(base),
    "process.env.PUBLIC_DEV": JSON.stringify(String(development)),
    "process.env.PUBLIC_MULTIPLAYER_URL": JSON.stringify(multiplayerUrl),
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
  let bakeWorkers = false;
  /** The texture-bake workers, whose URLs `texture-bake.ts` reads from `process.env`. */
  const buildBakeWorkers = async () => {
    for (const [source, key] of [
      ["src/game/texture-bake-worker.ts", "PUBLIC_WEBGPU_BAKE_WORKER_URL"],
      ["src/game/texture-bake-worker-webgl.ts", "PUBLIC_WEBGL_BAKE_WORKER_URL"],
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
      define[`process.env.${key}`] = JSON.stringify(base + name);
    }
    bakeWorkers = true;
  };
  // The game and room client start texture-bake workers, so build those first. Labs and
  // fixture pages stay independent of both generated gameplay engines unless their
  // bundle reaches the bakes (checked after it is built).
  if (entrypoints.some((entry) => entry === "src/game.ts" || entry === "src/net/client.ts"))
    await buildBakeWorkers();

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
  const templates = new Map<string, string>();
  for (const page of pages) {
    let html = await readFile(path.join(root, page), "utf8");
    // Public URLs retain their names and follow the deployment base.
    html = html.replace(/(\b(?:src|href)=["'])\/(?!\/)/g, `$1${base}`);
    html = devSiteHtml(html, base);
    html = html.replace(
      /<script\b([^>]*)>([\s\S]*?)<\/script>/gi,
      (tag, attrs: string, contents: string) => {
        if (!/\btype=["']module["']/.test(attrs)) return tag;
        const src = /\bsrc=["']([^"']+)["']/.exec(attrs)?.[1];
        // Bun's dev server leaves inline modules unbundled, so pages keep them in files.
        if (!src || contents.trim()) throw new Error(`${page}: move inline modules into a file`);
        const entry = path.posix.normalize(path.posix.join(path.posix.dirname(page), src));
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
    const bundle = () =>
      Bun.build({
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
        plugins: [browserAssets(cssAssets, files, base)],
      });
    let result = await bundle();
    // A bundle that reaches the bakes another way would keep `process.env` lookups, which
    // fail in browsers: give it the worker URLs and build it again.
    const reachesBakes = Object.keys(result.metafile?.inputs ?? {}).some(
      (file) => sourcePath(file) === "src/game/texture-bake.ts",
    );
    if (!bakeWorkers && reachesBakes) {
      await buildBakeWorkers();
      result = await bundle();
    }
    if (!result.metafile) throw new Error("Missing browser build metadata");
    const graph = sourceMetafile(result.metafile);
    Object.assign(metadata.inputs, graph.inputs);
    for (const [name, output] of Object.entries(graph.outputs)) {
      const relative = name.replace(/^\.\//, "");
      metadata.outputs[relative] = output;
      if (output.entryPoint) entries.set(output.entryPoint, relative);
    }
    for (const output of result.outputs) files.set(output.path.replace(/^\.\//, ""), output);
  }
  /** The stylesheets `file` loads before it runs: its own and its static imports'. */
  const stylesOf = (file: string, seen = new Set<string>()): string[] => {
    if (seen.has(file)) return [];
    seen.add(file);
    const output = metadata.outputs[file];
    const own = Object.keys(output?.inputs ?? {}).flatMap((input) => {
      const asset = cssAssets.get(path.join(root, input));
      return asset ? [base + asset.name] : [];
    });
    const imported = (output?.imports ?? [])
      .filter((dependency) => dependency.kind === "import-statement")
      .flatMap((dependency) => stylesOf(dependency.path.replace(/^\.\//, ""), seen));
    return [...new Set([...own, ...imported])];
  };
  /** An import of the lazy module `file` that starts its stylesheets beside its code,
   * instead of when the module has downloaded and runs. */
  const withStyles = (file: string) => {
    const load = `import(${JSON.stringify(base + file)})`;
    const styles = stylesOf(file);
    return styles.length
      ? `Promise.all([${load},${styles.map(styleLoad).join(",")}]).then(loaded=>loaded[0])`
      : load;
  };
  for (const [file, output] of Object.entries(metadata.outputs)) {
    const lazy = output.imports
      .filter((dependency) => dependency.kind === "dynamic-import")
      .map((dependency) => dependency.path.replace(/^\.\//, ""))
      .filter((target) => stylesOf(target).length);
    if (!lazy.length) continue;
    let code = await files.get(file)!.text();
    for (const target of lazy) {
      const load = `import(${JSON.stringify(base + target)})`;
      if (!code.includes(load)) throw new Error(`${file} imports ${target} in an unknown form`);
      code = code.replaceAll(load, withStyles(target));
    }
    files.set(file, new Blob([code], { type: "text/javascript;charset=utf-8" }));
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
        lazyEntries.map((entry) => [entry.id, withStyles(entries.get(entry.source)!)]),
      );
      const script = (await js.text()).replace(
        /import\((["'`])(sloppy:\w+)\1\)/g,
        (_load, _quote, id: string) => imports.get(id)!,
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
