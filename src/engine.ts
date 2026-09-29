// The Rust engine for the page: the wasm-bindgen glue and its hashed binary, built
// into `src/generated/engine/` by `pnpm run wasm`. Production pages start the
// binary's download from an inline <head> script (the engine-download plugin in
// vite.config.ts); this module takes that response over, so the download and
// compilation overlap with the menu and the engine's own JavaScript.
import init, { Game } from "./generated/engine/engine.js";
import binaryUrl from "./generated/engine/engine_bg.wasm?url";
import { WebGPUUnavailableError } from "./game/startup-error";

export { Game };

declare global {
  interface Window {
    /** Started by an inline <head> script; see the engine-download plugin. */
    sloppyEngineBinary?: Promise<Response>;
  }
}

let ready: Promise<void> | undefined;

/** Use the page's early download once; a retry after a failure fetches again. */
function download(): Promise<Response> {
  const early = window.sloppyEngineBinary;
  delete window.sloppyEngineBinary;
  return early ?? fetch(binaryUrl);
}

/** Download and instantiate the engine once. A failure lets a later call retry. */
export function loadEngine(): Promise<void> {
  ready ??= (async () => {
    const response = await download();
    if (!response.ok) {
      throw new Error(`Engine download failed: HTTP ${response.status}`);
    }
    // wasm-bindgen compiles a Response with instantiateStreaming when it is served
    // as application/wasm, and falls back to an ArrayBuffer otherwise.
    await init({ module_or_path: response });
  })();
  ready.catch(() => {
    ready = undefined;
  });
  return ready;
}

/** Create the engine's single-player game on `canvas`. WebGPU failures become
 * `WebGPUUnavailableError`, which the menu explains instead of offering a retry. */
export async function createGame(canvas: HTMLCanvasElement, config: object): Promise<Game> {
  if (!(navigator as { gpu?: unknown }).gpu) {
    throw new WebGPUUnavailableError(new Error("navigator.gpu is missing"));
  }
  await loadEngine();
  try {
    return await Game.create(canvas, JSON.stringify(config));
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (/^WebGPU (adapter|device|canvas) unavailable/.test(message)) {
      throw new WebGPUUnavailableError(error);
    }
    throw error;
  }
}
