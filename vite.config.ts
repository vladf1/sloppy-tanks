import { defineConfig, type Plugin, type Rolldown } from "vite";
import { statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { baseRedirect } from "./scripts/base-redirect.ts";
import { hashedAssets } from "./scripts/hashed-assets.ts";
import { pageHealth } from "./scripts/page-health.ts";

const base = process.env.DEPLOY_BASE || "/sloppy-tanks/";
const gamePage = fileURLToPath(new URL("./index.html", import.meta.url));

/** The shell binaries `index.html` may download, WebGPU first. */
const SHELL_BINARIES = [
  "src/generated/shell/sloppy-tanks_bg.wasm",
  "src/generated/shell-webgl/sloppy-tanks-webgl_bg.wasm",
];

/**
 * The shell binaries' sizes in bytes, as `<meta name="sloppy-shell-bytes">`, for the
 * game page's download progress (`index.html`): GitHub Pages sends them compressed, so
 * a response's length is not what it delivers. Read at every transform, so the dev
 * server follows `bun run wasm`; 0 for a binary not built.
 */
function shellSizes(): Plugin {
  const size = (file: string) => {
    try {
      return statSync(fileURLToPath(new URL(file, import.meta.url))).size;
    } catch {
      return 0;
    }
  };
  return {
    name: "shell-sizes",
    transformIndexHtml(_html, context) {
      if (context.filename !== gamePage) return [];
      return [
        {
          tag: "meta",
          attrs: { name: "sloppy-shell-bytes", content: SHELL_BINARIES.map(size).join(",") },
          injectTo: "head",
        },
      ];
    },
  };
}

/**
 * The game page's boot script (`index.html`) stays in the built page instead of a chunk
 * of its own, so a first visit starts the shell's and the fonts' downloads a round trip
 * sooner. Its imports, relative to the chunk's folder, become absolute.
 */
function inlineBootScript(): Plugin {
  let base = "/";
  return {
    name: "inline-boot-script",
    apply: "build",
    enforce: "post",
    configResolved(config) {
      base = config.base;
    },
    // After Vite has finished the chunks (it settles their preload calls last).
    generateBundle: {
      order: "post",
      handler: (_options, bundle) => inlineBootChunk(bundle, base),
    },
  };
}

/** Move the game page's boot chunk into its `<script>` (`inlineBootScript`). */
function inlineBootChunk(bundle: Rolldown.OutputBundle, base: string) {
  const page = bundle["index.html"];
  if (page?.type !== "asset") return;
  const html = String(page.source);
  const tag = html.match(/<script type="module" crossorigin src="([^"]+)"><\/script>/);
  const entry = Object.values(bundle).find(
    (file) => file.type === "chunk" && tag?.[1] === base + file.fileName,
  );
  if (!tag || entry?.type !== "chunk") throw new Error("index.html has no boot chunk");
  for (const file of Object.values(bundle)) {
    if (file.type === "chunk" && file.imports.includes(entry.fileName)) {
      throw new Error(`${file.fileName} imports the boot chunk`);
    }
  }
  const folder = entry.fileName.slice(0, entry.fileName.lastIndexOf("/") + 1);
  let code = inlinePreloadHelper(bundle, entry, folder);
  for (const file of entry.dynamicImports) {
    code = code.replaceAll(`./${file.slice(folder.length)}`, base + file);
  }
  const script = `<script type="module">${code.replaceAll("</script", "<\\/script")}</script>`;
  page.source = html.replace(tag[0], () => script);
  delete bundle[entry.fileName];
}

/** Vite still wraps dynamic imports for CSS with modulePreload disabled. A build with
 * fixtures shares that wrapper in its own chunk. Keep a copy in the boot script so
 * the game's initial downloads can start as soon as the HTML arrives. */
function inlinePreloadHelper(
  bundle: Rolldown.OutputBundle,
  entry: Rolldown.OutputChunk,
  folder: string,
): string {
  if (entry.imports.length === 0) return entry.code;
  const [file] = entry.imports;
  const helper = bundle[file];
  if (
    entry.imports.length !== 1 ||
    helper?.type !== "chunk" ||
    helper.name !== "preload-helper" ||
    helper.imports.length ||
    helper.dynamicImports.length
  ) {
    throw new Error("The game boot script has an unexpected static dependency");
  }

  // Rolldown assigns different local names in each chunk. Keep the helper in
  // its own scope and bind its export to the name used by the boot chunk.
  const imported = entry.code.match(
    /\bimport\s*\{\s*([\w$]+)(?:\s+as\s+([\w$]+))?\s*\}\s*from\s*["']([^"']+)["'];?/,
  );
  const exported = helper.code.match(/\bexport\s*\{\s*([\w$]+)(?:\s+as\s+([\w$]+))?\s*\};?\s*$/);
  if (
    !imported ||
    !exported ||
    imported[3] !== "./" + file.slice(folder.length) ||
    imported[1] !== (exported[2] ?? exported[1])
  ) {
    throw new Error("Could not inline Vite's preload helper in the game boot script");
  }
  const code = helper.code.slice(0, exported.index);
  const binding = imported[2] ?? imported[1];
  return entry.code.replace(
    imported[0],
    () => `const ${binding}=(()=>{${code}\nreturn ${exported[1]}})();`,
  );
}

export default defineConfig({
  base,
  // Preview launchers assign a free port through PORT; Vite does not read it itself.
  server: { port: Number(process.env.PORT) || undefined },
  plugins: [
    baseRedirect(base),
    pageHealth(),
    shellSizes(),
    hashedAssets(),
    inlineBootScript(),
    {
      // Vite content-hashes every file in /assets/, so the site's Worker
      // (deploy/site/) may let browsers cache those for good; HTML and unversioned
      // public files keep its default revalidation.
      name: "site-headers",
      generateBundle() {
        this.emitFile({
          type: "asset",
          fileName: "_headers",
          source: "/assets/*\n  Cache-Control: public, max-age=31536000, immutable\n",
        });
      },
    },
  ],
  // The bake worker (src/bake-worker.ts) imports the page's glue on demand.
  worker: { format: "es" },
  build: {
    // The pages' dynamic imports have no JavaScript dependencies to preload.
    modulePreload: false,
    rolldownOptions: {
      input: {
        main: gamePage,
      },
    },
  },
});
