import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";
import wasm from "vite-plugin-wasm";

const base = process.env.DEPLOY_BASE ?? "/sloppy-tanks/";

export default defineConfig({
  base,
  resolve: {
    alias: [
      {
        find: /^@dimforge\/rapier3d-compat$/,
        replacement: fileURLToPath(new URL("./src/game/physics-browser.ts", import.meta.url)),
      },
    ],
  },
  plugins: [
    wasm(),
    {
      name: "preload-physics",
      transformIndexHtml: {
        order: "post",
        handler(_html, context) {
          const binary = Object.keys(context.bundle ?? {}).find((name) => name.endsWith(".wasm"));
          return binary
            ? [
                {
                  tag: "link",
                  attrs: {
                    rel: "preload",
                    as: "fetch",
                    type: "application/wasm",
                    crossorigin: "anonymous",
                    href: `${base}${binary}`,
                  },
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
