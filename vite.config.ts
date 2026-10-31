import { defineConfig, type Plugin } from "vite";
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
 * server follows `pnpm run wasm`; 0 for a binary not built.
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

export default defineConfig({
  base,
  // Preview launchers assign a free port through PORT; Vite does not read it itself.
  server: { port: Number(process.env.PORT) || undefined },
  plugins: [baseRedirect(base), pageHealth(), shellSizes(), hashedAssets()],
  // The bake worker (src/bake-worker.ts) imports the page's glue on demand.
  worker: { format: "es" },
  build: {
    rolldownOptions: {
      input: {
        main: gamePage,
      },
    },
  },
});
