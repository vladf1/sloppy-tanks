import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";
import { lazyRapierWasm } from "./scripts/lazy-rapier-wasm.ts";
import { startupHtml } from "./scripts/startup-html.ts";

const base = process.env.DEPLOY_BASE ?? "/sloppy-tanks/";

export default defineConfig({
  base,
  // Preview launchers assign a free port through PORT; Vite does not read it itself.
  server: { port: Number(process.env.PORT) || undefined },
  resolve: {
    alias: [
      {
        find: /^@dimforge\/rapier3d-compat$/,
        replacement: fileURLToPath(new URL("./src/game/physics-browser.ts", import.meta.url)),
      },
    ],
  },
  plugins: [
    lazyRapierWasm(),
    startupHtml(base),
    {
      // Safari never hands a <link rel=preload as=fetch> response to a later
      // fetch(), so it downloaded the physics binary twice. Start the one real
      // request in <head>; physics-browser.ts takes it over in init().
      name: "physics-download",
      transformIndexHtml: {
        order: "post",
        handler(_html, context) {
          const binary = Object.keys(context.bundle ?? {}).find((name) => name.endsWith(".wasm"));
          return binary &&
            (context.filename.endsWith("index.html") ||
              context.filename.endsWith("stresstest.html"))
            ? [
                {
                  tag: "script",
                  children: `window.sloppyPhysicsBinary=fetch(${JSON.stringify(`${base}${binary}`)});window.sloppyPhysicsBinary.catch(()=>{});`,
                  injectTo: "head",
                },
              ]
            : [];
        },
      },
    },
  ],
  optimizeDeps: { exclude: ["@dimforge/rapier3d"] },
  build: {
    rolldownOptions: {
      input: {
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
        stresstest: fileURLToPath(new URL("./stresstest.html", import.meta.url)),
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
