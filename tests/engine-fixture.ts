// Shared setup for the `tests/*.browser.html` fixtures: one wasm `Game` on the page's
// canvas, arranged and drawn through its dev fixture hooks (`Game.debug_*`,
// `crates/web/src/game/debug.rs`). Fixtures with a verdict write it to `#result`
// starting with PASS or FAIL, which `scripts/fixtures-check.mjs` reads.
import { createGame, type Game } from "../src/engine";
import type { GameOptions } from "../src/game/game-options";
import { nextPrepareStep } from "../src/game/task-yield";

export type { Game };

/** Pipelines compiled per preparation step; the page stays responsive between steps. */
const PREPARE_BUDGET = 8;

export interface FixtureConfig {
  seed?: number;
  map?: string;
  gameMode?: "team" | "solo";
  humanKind?: "scout" | "balanced" | "heavy";
  humanTeam?: 0 | 1;
}

/** Create the engine on `canvas` at the window's size and prepare its arena. */
export async function fixtureGame(canvas: HTMLCanvasElement, config: FixtureConfig = {}) {
  const game = await createGame(canvas, {
    assetBase: import.meta.env.BASE_URL,
    cssWidth: innerWidth,
    cssHeight: innerHeight,
    pixelRatio: 1,
    ...config,
  });
  addEventListener("resize", () => game.resize(innerWidth, innerHeight, 1, false));
  await prepare(game);
  return game;
}

/** Compile the arena's pipelines and wait for its textures, yielding between steps. */
export async function prepare(game: Game): Promise<void> {
  let gpuPending = false;
  for (;;) {
    await nextPrepareStep(gpuPending);
    const [, , , done, pending] = game.prepare_step(PREPARE_BUDGET);
    if (done) {
      return;
    }
    gpuPending = pending === 1;
  }
}

/** Battle Setup's choices as the engine holds them. */
export function options(game: Game): GameOptions {
  const hud = JSON.parse(game.hud_json());
  return {
    humanKind: hud.human.kind,
    humanTeam: hud.humanTeam,
    gameMode: hud.gameMode,
    mapMode: hud.mapMode,
    difficulty: hud.difficulty,
  };
}

/** Rebuild the arena with changed choices and prepare it; returns whether it rebuilt. */
export async function choose(game: Game, changes: Partial<GameOptions>): Promise<boolean> {
  const rebuilt = game.set_options(JSON.stringify({ ...options(game), ...changes }));
  await prepare(game);
  return rebuilt;
}

export interface Vec3Pose {
  position: [number, number, number];
  target: [number, number, number];
}

/** Draw one frame; `camera` redraws it from a fixed pose. */
export function draw(game: Game, dt = 0, overview = false, camera?: Vec3Pose): void {
  const pose = camera ? [...camera.position, ...camera.target] : [];
  game.debug_render(1, dt, overview, new Float32Array(pose));
}

/** Parsed debug reports. */
export const read = {
  state: (game: Game) => JSON.parse(game.debug_json()),
  view: (game: Game) => JSON.parse(game.debug_view_json()),
  covers: (game: Game) => JSON.parse(game.debug_covers_json()),
  stats: (game: Game) => JSON.parse(game.stats_json()),
  hud: (game: Game) => JSON.parse(game.hud_json()),
};

export function check(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

/** Copy the WebGPU canvas's current frame to 2D pixels. Call in the task that drew it. */
export function canvasPixel(canvas: HTMLCanvasElement, x: number, y: number): number[] {
  const copy = document.createElement("canvas");
  copy.width = canvas.width;
  copy.height = canvas.height;
  const context = copy.getContext("2d", { willReadFrequently: true })!;
  context.drawImage(canvas, 0, 0);
  return Array.from(context.getImageData(Math.floor(x), Math.floor(y), 1, 1).data);
}
