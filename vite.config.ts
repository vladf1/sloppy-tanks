import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";
import { baseRedirect } from "./scripts/base-redirect.ts";
import { pageHealth } from "./scripts/page-health.ts";
import { startupHtml } from "./scripts/startup-html.ts";

const base = process.env.DEPLOY_BASE ?? "/sloppy-tanks/";

/** The inline <head> script: `window.sloppyGraphics` and `window.sloppyEngineBinary`
 * (declared in src/engine.ts). */
function engineDownloadScript(webgpuUrl: string, webglUrl: string): string {
  return [
    "(()=>{",
    "const g=/[?&]webgl(=|&|$)/.test(location.search)?null:navigator.gpu;",
    "const a=new AbortController();",
    `const w=g&&fetch(${JSON.stringify(webgpuUrl)},{signal:a.signal});`,
    'const api=g?g.requestAdapter().then(d=>d?"webgpu":"webgl",()=>"webgl"):Promise.resolve("webgl");',
    `const b=api.then(k=>k==="webgpu"?w:(a.abort(),fetch(${JSON.stringify(webglUrl)})));`,
    "w&&w.catch(()=>{});b.catch(()=>{});",
    "window.sloppyGraphics=api;window.sloppyEngineBinary=b;",
    "})();",
  ].join("");
}

export default defineConfig({
  base,
  // Preview launchers assign a free port through PORT; Vite does not read it itself.
  server: { port: Number(process.env.PORT) || undefined },
  plugins: [
    baseRedirect(base),
    startupHtml(base),
    pageHealth(),
    {
      // Start the engine binary's one real request in <head>, in parallel with the
      // inline menu and the engine's JavaScript; src/engine.ts takes it over. Safari
      // never hands a <link rel=preload as=fetch> response to a later fetch(), so a
      // preload link would download the binary twice there. The script also picks the
      // build, as src/engine.ts does: WebGPU when the browser has an adapter, else the
      // WebGL fallback. Waiting for the adapter would delay the common case, so a
      // browser with `navigator.gpu` starts the WebGPU download at once and aborts it
      // if no adapter comes.
      name: "engine-download",
      transformIndexHtml: {
        order: "post",
        handler(_html, context) {
          // By the source each binary was built from: the dev build also bundles the
          // labs engine (src/generated/engine-labs/engine_bg.wasm) for its test pages,
          // whose file name looks just like the game's.
          const emitted = (source: string) => {
            const files = Object.values(context.bundle ?? {}).filter(
              (output) =>
                output.type === "asset" &&
                output.originalFileNames.some((name) => name.endsWith(source)),
            );
            if (files.length > 1) {
              throw new Error(`More than one bundled ${source}`);
            }
            return files[0]?.fileName;
          };
          const webgpu = emitted("src/generated/engine/engine_bg.wasm");
          const webgl = emitted("src/generated/engine-webgl/engine-webgl_bg.wasm");
          return webgpu && webgl && context.filename.endsWith("index.html")
            ? [
                {
                  tag: "script",
                  children: engineDownloadScript(`${base}${webgpu}`, `${base}${webgl}`),
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
