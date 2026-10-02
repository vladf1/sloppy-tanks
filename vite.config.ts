import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";
import { pageHealth } from "./scripts/page-health.ts";
import { startupHtml } from "./scripts/startup-html.ts";

const base = process.env.DEPLOY_BASE ?? "/sloppy-tanks/";

export default defineConfig({
  base,
  // Preview launchers assign a free port through PORT; Vite does not read it itself.
  server: { port: Number(process.env.PORT) || undefined },
  plugins: [
    startupHtml(base),
    pageHealth(),
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
  build: {
    rolldownOptions: {
      input: {
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
      },
      output: {
        codeSplitting: {
          groups: [{ name: "vendor", test: /[\\/]node_modules[\\/]/ }],
        },
      },
    },
  },
});
