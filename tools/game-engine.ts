// The game's engine (`sloppy-web`; `pnpm run wasm:engine` writes it to
// `src/generated/engine/`) on a page of its own, without the shell: the game preview and
// the `tests/*.browser.html` fixtures create and prepare its `Game` through here.
import init, { Game } from "../src/generated/engine/engine.js";
import wasmUrl from "../src/generated/engine/engine_bg.wasm?url";

export type { Game };

/** Slots of the packed raw input frame (`sloppy_render::presentation::input::slot`).
 * Booleans are 0 or 1; one-shot slots count presses since the previous frame. */
export const INPUT = {
  up: 0,
  down: 1,
  left: 2,
  right: 3,
  touchMoveX: 4,
  touchMoveZ: 5,
  fire: 6,
  mine: 7,
  ammoSlot: 8,
  ammoStep: 9,
  pointerX: 10,
  pointerY: 11,
  stickTurn: 12,
  lookPixels: 13,
  zoom: 14,
  toggleView: 15,
  wheelAmmo: 16,
  length: 17,
} as const;

/** The slots of `Game.frame`'s result the preview reads (`frame_slot` in
 * `crates/web/src/game.rs`). */
export const FRAME = { phase: 0, humanAlive: 1, events: 4, clearInput: 5 } as const;

export const PHASES = ["ready", "playing", "paused", "results"] as const;

/** How often preparation polls a GPU that is still compiling pipelines in the background
 * or running the warm-up. */
const GPU_POLL_MS = 16;

let loading: Promise<unknown> | undefined;

/** Create the engine's single-player game on `canvas` from `Game.create`'s configuration. */
export async function createGame(canvas: HTMLCanvasElement, config: object): Promise<Game> {
  loading ??= init({ module_or_path: wasmUrl });
  await loading;
  return Game.create(canvas, JSON.stringify(config));
}

/** Resolve in a new message task, so the browser can paint first. Unlike a timer, a
 * message task is not throttled while the tab is in the background. */
function nextTask(): Promise<void> {
  const { port1, port2 } = new MessageChannel();
  return new Promise((resolve) => {
    port1.onmessage = () => {
      port1.close();
      resolve();
    };
    port2.postMessage(null);
  });
}

/** Compile the arena's pipelines `budget` at a time and wait for its textures, yielding
 * between steps so the page paints; `progress` sees each `prepare_step` result. While
 * only the GPU works it polls on a short timer, which can take seconds on a cold shader
 * cache and should not spin the page. */
export async function prepareArena(
  game: Game,
  budget: number,
  progress?: (step: Float64Array) => void,
): Promise<void> {
  let gpuPending = false;
  for (;;) {
    await (gpuPending ? new Promise((resolve) => setTimeout(resolve, GPU_POLL_MS)) : nextTask());
    const step = game.prepare_step(budget);
    progress?.(step);
    if (step[3]) {
      return;
    }
    gpuPending = step[4] === 1;
  }
}
