// The dev server's bundler plugin (bunfig.toml `[serve.static] plugins`). Bun's dev server
// bundles each page's <script> and <link> tags itself (`startDevServer` in site-server.ts);
// this fills in what the page markup leaves to the build, as site-build.ts does for
// production.
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import path from "node:path";
import type { BunPlugin } from "bun";
import { devSiteHtml } from "./dev-site.ts";
import { battleSetupHtml } from "./startup-html.ts";

const root = path.resolve(import.meta.dir, "..");

/** `page` (a file in the checkout) as the dev server bundles it. Root-absolute URLs name
 * public/ files: Bun reads the ones it bundles (stylesheets, icons, scripts) from there,
 * by a path from the page, and links follow the base. The game loads its startup module,
 * where production inlines a built copy. */
export async function devPageHtml(
  html: string,
  page: string,
  base: string,
  multiplayerUrl: string,
): Promise<string> {
  const publicFiles = path.relative(path.dirname(page), path.join(root, "public"));
  const publicPrefix = `${publicFiles.startsWith(".") ? "" : "./"}${publicFiles}/`;
  html = html
    .replace(/(<a\b[^>]*?\bhref=["'])\/(?!\/)/g, (_match, start: string) => start + base)
    .replace(
      /(<(?:link|script|img)\b[^>]*?\b(?:href|src)=["'])\/(?!\/)/g,
      (_match, start: string) => start + publicPrefix,
    );
  html = devSiteHtml(html, base);
  if (html.includes("<!-- battle-setup -->")) {
    const setup = await battleSetupHtml(base, multiplayerUrl);
    html = html.replace("<!-- battle-setup -->", () => setup);
  }
  return html.replace(
    "<!-- startup-script -->",
    '<script type="module" src="./src/main.ts"></script>',
  );
}

/** `source` with the build values it reads (src/build-env.d.ts) written in, or undefined
 * when it reads none. Bun's `[serve.static] env` inlines only the environment the process
 * started with, and its dev server takes no `define`. */
export function withBuildValues(
  source: string,
  values: Record<string, string | undefined>,
): string | undefined {
  if (!source.includes("process.env.PUBLIC_")) return undefined;
  return source.replace(/process\.env\.(PUBLIC_\w+)/g, (_read, key: string) => {
    const value = values[key];
    if (value === undefined) throw new Error(`The dev server sets no ${key}`);
    return JSON.stringify(value);
  });
}

const plugin: BunPlugin = {
  name: "sloppy-dev-pages",
  setup(build) {
    // startDevServer sets these before the first page is bundled.
    const base = process.env.PUBLIC_BASE_URL ?? "/";
    const multiplayerUrl = process.env.PUBLIC_MULTIPLAYER_URL ?? "";
    build.onLoad({ filter: /\/(src|tools|tests)\/[^/].*\.(ts|js)$/ }, async (args) => {
      const contents = withBuildValues(await readFile(args.path, "utf8"), process.env);
      if (contents === undefined) return undefined;
      return { contents, loader: args.path.endsWith(".js") ? "js" : "ts" };
    });
    build.onLoad({ filter: /\.html$/ }, async (args) => ({
      contents: await devPageHtml(
        await readFile(args.path, "utf8"),
        args.path,
        base,
        multiplayerUrl,
      ),
      loader: "html",
    }));
    // Engine binaries keep their names, served from src/generated/ by startDevServer,
    // instead of a copy of several megabytes in every bundle.
    build.onResolve({ filter: /\.wasm\?url$/ }, (args) => ({
      path: path.resolve(args.resolveDir, args.path.slice(0, -"?url".length)),
      namespace: "engine-binary",
    }));
    build.onLoad({ filter: /.*/, namespace: "engine-binary" }, (args) => ({
      contents: `export default ${JSON.stringify(base + path.relative(root, args.path))};`,
      loader: "js",
    }));
    // A lab page explains a missing labs build (tools/labs-engine.ts) instead of failing
    // to bundle.
    build.onResolve({ filter: /\/generated\/engine-labs\/engine-labs\.js$/ }, (args) => {
      const file = path.resolve(args.resolveDir, args.path);
      return existsSync(file) ? undefined : { path: file, namespace: "missing-labs" };
    });
    build.onLoad({ filter: /.*/, namespace: "missing-labs" }, () => ({
      contents: 'throw new Error("The labs engine is not built: run `bun run wasm:labs`");',
      loader: "js",
    }));
  },
};

export default plugin;
