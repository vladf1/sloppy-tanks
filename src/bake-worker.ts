// A worker that bakes one band of a generated texture for the shell
// (`crates/desktop/src/bakes.rs`), off the page's main thread: it runs its own instance
// of the module the page compiled, whose `main` returns at once in a worker, and calls
// only its `bake_texture`. The page starts it (`sloppyBakeWorker` in `index.html`) and
// first tells it where its build's glue is (`shell-glue.ts`), which the page has
// downloaded already, and gives it the engine half of the binary
// (`scripts/split-wasm.mjs`), which holds the bakes: the worker instantiates it over its
// own menu half's exports, as the page does.

import type * as Glue from "./shell-glue.ts";

interface GlueAddress {
  glue: string;
  engine?: WebAssembly.Module;
}

interface BakeRequest {
  /** The page's compiled shell, so the worker downloads and compiles nothing. */
  module: WebAssembly.Module;
  key: string;
  band: number;
  bands: number;
}

let glueUrl = "";
let engine: WebAssembly.Module | undefined;

onmessage = async ({ data }: MessageEvent<GlueAddress | BakeRequest>) => {
  if ("glue" in data) {
    glueUrl = data.glue;
    engine = data.engine;
    return;
  }
  try {
    const glue: typeof Glue = await import(/* @vite-ignore */ glueUrl);
    const exports = await glue.default({ module_or_path: data.module });
    if (!engine) {
      throw new Error("The page gave no engine half to bake with");
    }
    await WebAssembly.instantiate(engine, { primary: { ...exports } });
    const pixels = glue.bake_texture(data.key, data.band, data.bands);
    if (!pixels) {
      throw new Error(`The shell does not bake ${data.key}`);
    }
    postMessage(pixels, { transfer: [pixels.buffer] });
  } catch (error) {
    postMessage(String(error));
  }
};
