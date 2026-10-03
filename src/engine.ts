// The Rust engine for the page, single player and room alike: the wasm-bindgen glue
// and its hashed binary, built by `pnpm run wasm`. There are two builds: the WebGPU
// engine (`src/generated/engine/`) and a WebGL2 fallback
// (`src/generated/engine-webgl/`). A page loads one: the fallback only where the
// browser offers no WebGPU adapter (or `?webgl` asks for it), so WebGPU browsers
// never download WebGL code; its glue is a separate chunk imported on demand.
// Production pages make that choice and start the binary's download from an inline
// <head> script (the engine-download plugin in vite.config.ts); this module takes
// both over, so the download and compilation overlap with the menu and the engine's
// own JavaScript, and a page downloads and compiles the engine once.
import * as webgpuGlue from "./generated/engine/engine.js";
import type { Game, NetGame } from "./generated/engine/engine.js";
import webgpuBinaryUrl from "./generated/engine/engine_bg.wasm?url";
import webglBinaryUrl from "./generated/engine-webgl/engine-webgl_bg.wasm?url";
import { GraphicsUnavailableError } from "./game/startup-error";

export type { Game, NetGame };

/** The browser graphics API an engine build renders with. */
export type GraphicsApi = "webgpu" | "webgl";

declare global {
  interface Window {
    /** The <head> script's choice of engine build; see the engine-download plugin. */
    sloppyGraphics?: Promise<GraphicsApi>;
    /** Started by an inline <head> script for `sloppyGraphics`' build. */
    sloppyEngineBinary?: Promise<Response>;
  }
}

/** What the page calls on either build; both have the same Rust API. */
export interface EngineGlue {
  default(options: { module_or_path: WebAssembly.Module }): Promise<unknown>;
  Game: typeof Game;
  NetGame: typeof NetGame;
}

interface Engine {
  api: GraphicsApi;
  glue: EngineGlue;
  module: WebAssembly.Module;
}

const BINARY_URL: Record<GraphicsApi, string> = {
  webgpu: webgpuBinaryUrl,
  webgl: webglBinaryUrl,
};

/** The WebGL build's glue, imported only when the page falls back to it. */
async function webglGlue(): Promise<EngineGlue> {
  const glue: EngineGlue = await import("./generated/engine-webgl/engine-webgl.js");
  return glue;
}

/** WebGPU wherever the browser hands out an adapter; `?webgl` asks for the fallback.
 * Production pages run the same test in their <head> script first. */
function probeGraphics(): Promise<GraphicsApi> {
  const gpu = (navigator as { gpu?: { requestAdapter(): Promise<unknown> } }).gpu;
  if (!gpu || new URLSearchParams(location.search).has("webgl")) {
    return Promise.resolve("webgl");
  }
  return gpu.requestAdapter().then(
    (adapter) => (adapter ? "webgpu" : "webgl"),
    () => "webgl",
  );
}

let api: Promise<GraphicsApi> | undefined;
const loading: Partial<Record<GraphicsApi, Promise<Engine>>> = {};
let current: Engine | undefined;

/** The build this page renders with, decided once (or changed by a fallback). */
function graphicsApi(): Promise<GraphicsApi> {
  api ??= window.sloppyGraphics ?? probeGraphics();
  delete window.sloppyGraphics;
  return api;
}

/** Use the page's early download once: it is for the build the page loads first. A
 * retry after a failure, or a later fallback, fetches its own. */
function download(build: GraphicsApi): Promise<Response> {
  const early = window.sloppyEngineBinary;
  delete window.sloppyEngineBinary;
  return early ?? fetch(BINARY_URL[build]);
}

/** Download and instantiate `build` once. A failure lets a later call retry. */
function loadBuild(build: GraphicsApi): Promise<Engine> {
  const engine = (loading[build] ??= (async () => {
    const response = await download(build);
    if (!response.ok) {
      throw new Error(`Engine download failed: HTTP ${response.status}`);
    }
    // Compiled here rather than by wasm-bindgen so bake workers can instantiate the
    // same module (`texture-bake.ts`) without compiling it again.
    const module =
      response.headers.get("Content-Type") === "application/wasm"
        ? await WebAssembly.compileStreaming(response)
        : await WebAssembly.compile(await response.arrayBuffer());
    const glue = build === "webgpu" ? webgpuGlue : await webglGlue();
    await glue.default({ module_or_path: module });
    current = { api: build, glue, module };
    return current;
  })());
  engine.catch(() => {
    if (loading[build] === engine) {
      delete loading[build];
    }
  });
  return engine;
}

/** Download and instantiate the page's engine once. A failure lets a later call retry. */
export async function loadEngine(): Promise<EngineGlue> {
  return (await loadBuild(await graphicsApi())).glue;
}

/** The engine's compiled module once `loadEngine` has finished, and whether it is
 * the WebGL build, whose glue a bake worker must use. */
export function engineModule(): { module: WebAssembly.Module; webgl: boolean } {
  if (!current) {
    throw new Error("The engine has not loaded");
  }
  return { module: current.module, webgl: current.api === "webgl" };
}

/** An engine startup error that means its graphics API cannot run here. */
function graphicsUnavailable(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return /^(WebGPU|WebGL) (adapter|device|canvas) unavailable/.test(message);
}

/** Create an engine game. A WebGPU adapter can still fail to give a device; then the
 * page falls back to the WebGL build, on the same canvas (the engine claims it only
 * once it has a device). When neither API works the error is a
 * `GraphicsUnavailableError`, which the menu explains instead of offering a retry. */
async function createWith<T>(create: (glue: EngineGlue) => Promise<T>): Promise<T> {
  let engine = await loadBuild(await graphicsApi());
  if (engine.api === "webgpu") {
    try {
      return await create(engine.glue);
    } catch (error) {
      if (!graphicsUnavailable(error)) {
        throw error;
      }
      console.warn("WebGPU is unavailable; falling back to WebGL.", error);
      api = Promise.resolve("webgl");
      engine = await loadBuild("webgl");
    }
  }
  try {
    return await create(engine.glue);
  } catch (error) {
    throw graphicsUnavailable(error) ? new GraphicsUnavailableError(error) : error;
  }
}

/** Create the engine's single-player game on `canvas`. */
export function createGame(canvas: HTMLCanvasElement, config: object): Promise<Game> {
  return createWith((glue) => glue.Game.create(canvas, JSON.stringify(config)));
}

/** Create a room's game on `canvas` from `NetGame.create`'s configuration JSON. */
export function createNetGame(canvas: HTMLCanvasElement, configJson: string): Promise<NetGame> {
  return createWith((glue) => glue.NetGame.create(canvas, configJson));
}
