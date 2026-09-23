import { build } from "esbuild";
import { copyFile, mkdir, rename } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { contentVersion } from "../scripts/content-version.mjs";
const repo = (path) => fileURLToPath(new URL(`../${path}`, import.meta.url));
await mkdir(repo("server/dist"), { recursive: true });
await build({
  entryPoints: [repo("server/worker.ts")],
  outfile: repo("server/dist/worker.js"),
  bundle: true,
  format: "esm",
  platform: "neutral",
  mainFields: ["module", "main"],
  external: ["cloudflare:workers"],
  sourcemap: true,
  define: { __MULTIPLAYER_CONTENT_VERSION__: JSON.stringify(await contentVersion()) },
  plugins: [
    {
      name: "rapier-workerd",
      setup(context) {
        context.onResolve({ filter: /^@dimforge\/rapier3d-compat$/ }, () => ({
          path: repo("server/physics-workerd.ts"),
        }));
        context.onResolve({ filter: /^\.\/rapier\.wasm$/ }, () => ({
          path: "./rapier.wasm",
          external: true,
        }));
        // Rapier 0.20.0: wasm-bindgen's namespace and __wbg_set_wasm are private API.
        // Review this loader whenever either pinned Rapier package changes.
        context.onLoad({ filter: /rapier3d[\\/]rapier_wasm3d\.js$/ }, () => ({
          loader: "js",
          resolveDir: repo("node_modules/@dimforge/rapier3d"),
          contents: `import wasmModule from "./rapier.wasm";
        import * as bg from "./rapier_wasm3d_bg.js";
        const instance = new WebAssembly.Instance(wasmModule, { "./rapier_wasm3d_bg.js": bg });
        bg.__wbg_set_wasm(instance.exports);
        export * from "./rapier_wasm3d_bg.js";`,
        }));
      },
    },
  ],
});
// Wrangler watches this directory. Publish the binary atomically so a running
// local server cannot reload a partially copied WASM module.
await copyFile(
  repo("node_modules/@dimforge/rapier3d/rapier_wasm3d_bg.wasm"),
  repo("server/dist/rapier.wasm.tmp"),
);
await rename(repo("server/dist/rapier.wasm.tmp"), repo("server/dist/rapier.wasm"));
