// Game preview: single-player Sloppy Tanks on the Rust engine (simulation,
// presentation and the wgpu renderer), with minimal keyboard and mouse input and
// no menus or HUD. Build the engine with `pnpm run wasm`, then open
// /sloppy-tanks/tools/game-preview.html on the dev server.
// `?map=village|harbor|quarry|stress-test|superstress`, `?seed=424242`,
// `?autoplay`, `?fp` (start in first person), `?zoom=34`.
import { FRAME, INPUT, PHASES } from "../src/game/engine-api";
import { nextPrepareStep } from "../src/game/task-yield";
import init, { Game } from "../src/generated/engine/engine.js";
import wasmUrl from "../src/generated/engine/engine_bg.wasm?url";

const ZOOM_STEP = 2;
const PREPARE_BUDGET = 4;

const params = new URLSearchParams(location.search);
const canvas = document.querySelector<HTMLCanvasElement>("#game")!;
const status = document.querySelector<HTMLElement>("#status")!;

const keys = new Set<string>();
const input = new Float32Array(INPUT.length);
let mines = 0;
let ammoSlot = 0;
let ammoStep = 0;
let wheelAmmo = 0;
let look = 0;
let zoom = 0;
let toggleView = false;
let fire = false;

function bindInput(): void {
  addEventListener("keydown", (event) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const digit = /^(Digit|Numpad)([1-5])$/.exec(event.code);
    if (digit) {
      if (!event.repeat) ammoSlot = Number(digit[2]);
      event.preventDefault();
      return;
    }
    if (event.code === "KeyQ" || event.code === "KeyE") {
      if (!event.repeat) ammoStep = event.code === "KeyQ" ? -1 : 1;
      return;
    }
    if (event.code === "KeyV") {
      if (!event.repeat) toggleView = true;
      return;
    }
    if (
      event.code.startsWith("Arrow") ||
      /^Key[WASD]$/.test(event.code) ||
      event.code === "Space"
    ) {
      event.preventDefault();
      keys.add(event.code);
    }
  });
  addEventListener("keyup", (event) => keys.delete(event.code));
  addEventListener("blur", () => {
    keys.clear();
    fire = false;
  });
  canvas.addEventListener("pointermove", (event) => {
    const rect = canvas.getBoundingClientRect();
    input[INPUT.pointerX] = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    input[INPUT.pointerY] = 1 - ((event.clientY - rect.top) / rect.height) * 2;
    look += event.movementX ?? 0;
  });
  canvas.addEventListener("pointerdown", (event) => {
    canvas.focus();
    if (event.button === 0) fire = true;
    if (event.button === 2) mines++;
  });
  addEventListener("pointerup", (event) => {
    if (event.button === 0) fire = false;
  });
  canvas.addEventListener("contextmenu", (event) => event.preventDefault());
  canvas.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      if (event.shiftKey) {
        const delta = event.deltaY || event.deltaX;
        if (delta) zoom += Math.sign(delta) * ZOOM_STEP;
      } else if (event.deltaY) {
        wheelAmmo = event.deltaY > 0 ? 1 : -1;
      }
    },
    { passive: false },
  );
}

/** Raw control state for one frame; one-shot presses reset once sent. */
function takeInput(): Float32Array {
  const held = (...codes: string[]) => (codes.some((code) => keys.has(code)) ? 1 : 0);
  input[INPUT.up] = held("KeyW", "ArrowUp");
  input[INPUT.down] = held("KeyS", "ArrowDown");
  input[INPUT.left] = held("KeyA", "ArrowLeft");
  input[INPUT.right] = held("KeyD", "ArrowRight");
  input[INPUT.fire] = fire || keys.has("Space") ? 1 : 0;
  input[INPUT.mine] = mines;
  input[INPUT.ammoSlot] = ammoSlot;
  input[INPUT.ammoStep] = ammoStep;
  input[INPUT.wheelAmmo] = wheelAmmo;
  input[INPUT.lookPixels] = look;
  input[INPUT.zoom] = zoom;
  input[INPUT.toggleView] = toggleView ? 1 : 0;
  mines = ammoSlot = ammoStep = wheelAmmo = look = zoom = 0;
  toggleView = false;
  return input;
}

async function main(): Promise<void> {
  if (!navigator.gpu) throw new Error("WebGPU is required.");
  bindInput();
  await init({ module_or_path: wasmUrl });
  const seed = params.has("seed") ? Number(params.get("seed")) : 424242;
  const map = params.get("map") ?? "village";
  const game = await Game.create(
    canvas,
    JSON.stringify({
      seed,
      assetBase: import.meta.env.BASE_URL,
      map,
      extraLevels: true,
      autoplay: params.has("autoplay"),
      cssWidth: innerWidth,
      cssHeight: innerHeight,
      pixelRatio: devicePixelRatio,
    }),
  );
  status.textContent = `Preparing ${map}…`;
  let progress: Float64Array;
  let gpuPending = false;
  do {
    await nextPrepareStep(gpuPending);
    progress = game.prepare_step(PREPARE_BUDGET);
    gpuPending = progress[4] === 1;
    status.textContent = `Preparing ${map}: ${progress[1]} pipelines, ${progress[2]} textures left`;
  } while (!progress[3]);
  if (params.has("zoom")) game.debug_set_zoom(Number(params.get("zoom")));
  if (params.has("reflections")) game.debug_set_reflections(Number(params.get("reflections")));
  game.start();
  if (params.has("fp")) game.toggle_first_person();
  addEventListener("resize", () => game.resize(innerWidth, innerHeight, devicePixelRatio, false));
  let frames = 0;
  document.body.dataset.state = "playing";
  canvas.focus();
  const loop = (now: number) => {
    try {
      const result = game.frame(now, takeInput());
      frames++;
      if (result[FRAME.events] > 0) {
        game.drain_events();
      }
      if (result[FRAME.clearInput]) {
        fire = false;
      }
      if (frames % 30 === 0) {
        const stats = JSON.parse(game.stats_json());
        const phase = PHASES[result[FRAME.phase]];
        status.textContent =
          `${map} · ${phase}${result[FRAME.humanAlive] ? "" : " (destroyed)"} · ` +
          `${stats.fps.toFixed(0)} fps · ${stats.drawCalls} draws · ${stats.triangles} tris · ` +
          `sim ${stats.simMs.toFixed(2)} ms · render ${stats.renderMs.toFixed(2)} ms`;
      }
      const error = game.error();
      if (error) throw new Error(error);
      requestAnimationFrame(loop);
    } catch (error) {
      fail(error);
    }
  };
  requestAnimationFrame(loop);
}

function fail(error: unknown): void {
  document.body.dataset.state = "error";
  status.textContent = String(error instanceof Error ? error.message : error);
  console.error(error);
}

main().catch(fail);
