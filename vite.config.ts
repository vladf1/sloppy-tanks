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
      // Bundle Three from its WebGPU sources. r185's prebuilt three.webgpu.js
      // holds TSL in one frozen object that three/tsl reads every export from,
      // which kept every TSL function and node class; the sources export an ES
      // namespace that tree-shakes. Plain "three" would otherwise name the WebGL
      // build. All three entries must resolve to the same files, or Three loads twice.
      { find: /^three(\/webgpu)?$/, replacement: "three/src/Three.WebGPU.js" },
      // r185's Three.TSL.js re-exports names its node sources no longer define.
      { find: /^three\/tsl$/, replacement: "three/src/nodes/TSL.js" },
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
          return binary && context.filename.endsWith("index.html")
            ? [
                {
                  tag: "script",
                  children: `if(!new URLSearchParams(location.search).has("room")&&!new URLSearchParams(location.search).has("multiplayer")){window.sloppyPhysicsBinary=fetch(${JSON.stringify(`${base}${binary}`)});window.sloppyPhysicsBinary.catch(()=>{});}`,
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
