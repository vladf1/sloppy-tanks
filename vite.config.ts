import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";
import { lazyRapierWasm } from "./scripts/lazy-rapier-wasm.ts";
import { startupHtml } from "./scripts/startup-html.ts";
import { contentVersion } from "./scripts/content-version.mjs";

const base = process.env.DEPLOY_BASE ?? "/sloppy-tanks/";

export default defineConfig({
  base,
  define: { __MULTIPLAYER_CONTENT_VERSION__: JSON.stringify(await contentVersion()) },
  // Preview launchers assign a free port through PORT; Vite does not read it itself.
  server: { port: Number(process.env.PORT) || undefined },
  resolve: {
    alias: [
      {
        find: /^@dimforge\/rapier3d-simd-compat$/,
        replacement: fileURLToPath(new URL("./src/game/physics-browser.ts", import.meta.url)),
      },
    ],
  },
  plugins: [
    lazyRapierWasm(),
    startupHtml(base),
    {
      // Start the engine binary's one real request in <head>, in parallel with the
      // inline menu and the engine's JavaScript; src/engine.ts takes it over. Safari
      // never hands a <link rel=preload as=fetch> response to a later fetch(), so a
      // preload link would download the binary twice there.
      name: "engine-download",
      transformIndexHtml: {
        order: "post",
        handler(_html, context) {
          const binary = Object.keys(context.bundle ?? {}).find((name) =>
            /(^|\/)engine_bg-[\w-]+\.wasm$/.test(name),
          );
          return binary && context.filename.endsWith("index.html")
            ? [
                {
                  tag: "script",
                  children: `window.sloppyEngineBinary=fetch(${JSON.stringify(`${base}${binary}`)});window.sloppyEngineBinary.catch(()=>{});`,
                  injectTo: "head",
                },
              ]
            : [];
        },
      },
    },
  ],
  optimizeDeps: { exclude: ["@dimforge/rapier3d-simd"] },
  build: {
    rolldownOptions: {
      input: {
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
      },
      output: {
        codeSplitting: {
          groups: [
            { name: "physics", test: /[\\/]@dimforge[\\/]/, priority: 20 },
            { name: "graphics", test: /[\\/]three[\\/]/, priority: 10 },
            { name: "vendor", test: /[\\/]node_modules[\\/]/ },
          ],
        },
      },
    },
  },
});
