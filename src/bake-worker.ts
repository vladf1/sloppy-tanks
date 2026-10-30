// A worker that bakes one band of a generated texture for the shell
// (`crates/desktop/src/bakes.rs`), off the page's main thread: it runs its own instance
// of the module the page compiled, whose `main` returns at once in a worker, and calls
// only its `bake_texture`. The page starts it (`sloppyBakeWorker` in `index.html`).

interface BakeRequest {
  /** The page's compiled shell, so the worker downloads and compiles nothing. */
  module: WebAssembly.Module;
  /** Whether `module` is the WebGL build, whose glue it needs. */
  webgl: boolean;
  key: string;
  band: number;
  bands: number;
}

onmessage = async ({ data }: MessageEvent<BakeRequest>) => {
  try {
    const glue = data.webgl
      ? await import("./generated/shell-webgl/sloppy-tanks-webgl.js")
      : await import("./generated/shell/sloppy-tanks.js");
    await glue.default({ module_or_path: data.module });
    const pixels = glue.bake_texture(data.key, data.band, data.bands);
    if (!pixels) {
      throw new Error(`The shell does not bake ${data.key}`);
    }
    postMessage(pixels, { transfer: [pixels.buffer] });
  } catch (error) {
    postMessage(String(error));
  }
};
