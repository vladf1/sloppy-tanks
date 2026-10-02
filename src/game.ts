// Single player in the page: the Rust engine (`crates/web/src/game.rs`) owns the
// simulation, its fixed-step loop, presentation and rendering. This module keeps the
// browser side: preparing the arena behind Battle Setup, one engine frame per
// animation frame with the packed raw input, and engine events and HUD state for
// sound and the DOM.
import { createGame, engineModule, type Game } from "./engine";
import { FrameRecorder, createDebug } from "./diagnostics";
import { AudioSystem } from "./game/audio";
import { Cockpit } from "./game/cockpit";
import { Controls } from "./game/controls";
import {
  FRAME,
  INPUT,
  PHASES,
  type EngineStats,
  type EventBatch,
  type HudState,
  type Phase,
} from "./game/engine-api";
import { sameGameOptions, type GameOptions } from "./game/game-options";
import { isExtraLevel, showsExtraLevels } from "./game/map-options";
import { NerdStats, engineStatsSections } from "./game/nerd-stats";
import type { PreparedGame } from "./game/start-menu";
import { afterPaint, nextPrepareStep } from "./game/task-yield";
import { startTextureBake } from "./game/texture-bake";
import { TouchModeController } from "./game/touch-mode";
import { MENU_READY_STATUS, UI } from "./game/ui";
const HUD_UPDATE_EVERY_FRAMES = 4;
/** Pipelines compiled per preparation call; the menu stays responsive between calls. */
const PREPARE_BUDGET = 4;
/** How often the loop asks the renderer whether the GPU reported an error. */
const ERROR_CHECK_EVERY_FRAMES = 30;
/** `window.sloppy.exactResolution()` renders at this size whatever the window. */
const EXACT_RESOLUTION = { width: 2560, height: 1440 } as const;

/** Prepare a hidden arena after the lightweight menu has painted. */
export async function prepareGame(
  app: HTMLElement,
  seed: number,
  getOptions: () => GameOptions,
  onStage: (stage: string) => void = () => {},
): Promise<PreparedGame> {
  onStage("Building the arena…");
  const root = document.createElement("div");
  root.hidden = true;
  app.append(root);
  root.innerHTML =
    '<canvas id="game" tabindex="0" aria-label="Sloppy Tanks 3D demolition arena"></canvas>';
  const canvas = root.querySelector<HTMLCanvasElement>("#game")!;
  const params = new URLSearchParams(location.search);
  // The Battle Setup choices; the in-game menu edits this object too.
  const choices: GameOptions = { ...getOptions() };
  let game: Game;
  try {
    game = await createGame(canvas, {
      seed,
      assetBase: import.meta.env.BASE_URL,
      map: choices.mapMode,
      extraLevels: showsExtraLevels(location.search) || isExtraLevel(choices.mapMode),
      difficulty: choices.difficulty,
      humanKind: choices.humanKind,
      humanTeam: choices.humanTeam,
      gameMode: choices.gameMode,
      autoplay: params.has("autoplay"),
      cssWidth: innerWidth,
      cssHeight: innerHeight,
      pixelRatio: devicePixelRatio,
    });
  } catch (error) {
    root.remove();
    throw error;
  }
  // The options the engine's world was built with.
  let prepared: GameOptions = { ...choices };
  let wanted: GameOptions = prepared;
  let reportShaders = onStage;
  let arenaPreparation: Promise<void> | undefined;
  // A world in use is never rebuilt; BATTLE SETUP gives the menu a fresh one.
  let inRound = false;
  onStage("Preparing graphics…");

  /** Bring the hidden arena to `options` while the player is still choosing.
   * One preparation runs at a time; choices changed meanwhile are picked up by
   * its loop, and choices that already match cost nothing. */
  function prepareArena(options: GameOptions, report: (stage: string) => void): Promise<void> {
    wanted = options;
    reportShaders = report;
    if (!arenaPreparation) {
      // Cleared only after assignment: a run with nothing to do settles at once.
      const preparation = updateArena();
      const clear = () => {
        if (arenaPreparation === preparation) {
          arenaPreparation = undefined;
        }
      };
      arenaPreparation = preparation;
      preparation.then(clear, clear);
    }
    return arenaPreparation;
  }
  async function updateArena(): Promise<void> {
    let total = 0;
    for (;;) {
      if (inRound) {
        return;
      }
      if (!sameGameOptions(prepared, wanted)) {
        const next = { ...wanted };
        // The rebuild blocks the main thread for up to a few hundred milliseconds; show
        // the player's new choice, or GO's WAIT, before it starts.
        await afterPaint();
        if (inRound) {
          return;
        }
        if (!sameGameOptions(next, wanted)) {
          continue;
        }
        game.set_options(JSON.stringify(next));
        prepared = next;
        total = 0;
      }
      // Compile a few pipelines per task until the world's shaders, textures and
      // first frames are ready, or the choices change again.
      let gpuPending = false;
      for (;;) {
        await nextPrepareStep(gpuPending);
        if (inRound || !sameGameOptions(prepared, wanted)) {
          break;
        }
        startTextureBake(game, engineModule());
        const [compiled, remaining, texturesPending, done, waitingForGpu] =
          game.prepare_step(PREPARE_BUDGET);
        gpuPending = waitingForGpu === 1;
        total += compiled;
        if (done) {
          return;
        }
        // A rebuild that reuses every pipeline and texture still waits for its warm-up
        // frames; that is not loading, so the menu keeps showing it is ready.
        if (total === 0 && remaining === 0 && texturesPending === 0) {
          continue;
        }
        reportShaders(
          remaining > 0
            ? `Shaders loaded: ${total} of ${total + remaining}`
            : gpuPending
              ? "Compiling shaders…"
              : `Loading textures… ${texturesPending} left`,
        );
      }
    }
  }

  let active = false;
  let phase: Phase = "ready";
  let alive = true;
  let hud: HudState | undefined;
  const readHud = () => (hud = JSON.parse(game.hud_json()) as HudState);
  const stats = new NerdStats(
    root,
    () => engineStatsSections(JSON.parse(game.stats_json()) as EngineStats),
    () => active && !root.classList.contains("menu-ready"),
  );
  let pendingZoom = 0;
  const zoom = (amount: number) => {
    pendingZoom += amount;
  };
  const controls = new Controls(
    canvas,
    () => pause(),
    zoom,
    () => phase === "playing" && alive,
  );
  // Creating the AudioContext can block the main thread for over 150 ms. Sounds
  // are first needed when a round begins, so this waits until the menu is ready.
  let audioSystem: AudioSystem | undefined;
  let volume = 0;
  const audio = () => {
    if (!audioSystem) {
      audioSystem = new AudioSystem();
      audioSystem.volume(volume);
    }
    return audioSystem;
  };
  const settings = (key: string, value: number) => {
    if (key === "tank-speed" || key === "bullet-speed") {
      value = game.set_speed(key, value);
    }
    try {
      localStorage.setItem("sloppy-" + key, String(value));
    } catch {
      /* Session-only preference. */
    }
    if (key === "volume") {
      volume = value;
      audioSystem?.volume(value);
    }
  };
  const saved = (key: string, fallback: string) => {
    try {
      return Number(localStorage.getItem("sloppy-" + key) ?? fallback);
    } catch {
      return Number(fallback);
    }
  };
  settings("volume", saved("volume", ".6"));
  for (const key of ["tank-speed", "bullet-speed"] as const) {
    settings(key, saved(key, "1"));
  }

  // Whether the last frame steered in first person (toggling the view sets it too).
  let firstPerson = false;
  /** Pause, results and the round menu need the cursor; a death keeps it captured.
   * Applied as soon as the phase changes, so a click right after RESUME already
   * takes the pointer back instead of waiting for the next frame. */
  const holdPointer = () => controls.holdPointer(firstPerson, phase !== "playing");
  const updateHud = (dt: number) => {
    ui.update(readHud(), dt);
    phase = hud!.match.phase;
    alive = hud!.human.alive;
    holdPointer();
    touchControls.update();
  };
  function pause(): void {
    if (phase === "playing") {
      game.pause();
      controls.clear();
      updateHud(0);
    }
  }
  let roundStarting = false;
  async function startFromMenu(): Promise<void> {
    if (roundStarting) {
      return;
    }
    roundStarting = true;
    active = false;
    controls.clear();
    root.classList.add("menu-ready");
    const button = ui.overlay.querySelector<HTMLButtonElement>("#start, #play-again");
    if (button) {
      button.disabled = true;
      button.textContent = "WAIT";
    }
    ui.overlay.dataset.state = "starting";
    const status = ui.overlay.querySelector("#startup-status");
    if (status) {
      status.textContent = "Preparing your arena…";
    }
    try {
      if (phase === "results") {
        // PLAY AGAIN: the engine starts a fresh world with the same choices.
        beginRound();
        return;
      }
      await prepareArena(choices, (stage) => {
        if (status) {
          status.textContent = stage;
        }
      });
      beginRound();
    } catch (error) {
      console.error("Round preparation failed", error);
      ui.overlay.dataset.state = "error";
      if (status) {
        status.textContent = "The arena could not load. Please try again.";
      }
      if (button) {
        button.disabled = false;
        button.textContent = "TRY AGAIN";
      }
    } finally {
      roundStarting = false;
    }
  }
  /** Prepare the in-game battle setup's choices while the player is still choosing. */
  async function preloadFromMenu(): Promise<void> {
    if (roundStarting || inRound) {
      return;
    }
    try {
      await prepareArena(choices, (stage) => {
        delete ui.overlay.dataset.state;
        const status = ui.overlay.querySelector("#startup-status");
        if (status) {
          status.textContent = stage;
        }
      });
    } catch (error) {
      // GO prepares again and reports the failure.
      console.error("Arena preparation failed", error);
    }
    if (!roundStarting && !inRound && ui.overlay.dataset.state !== "ready") {
      ui.overlay.dataset.state = "ready";
      const status = ui.overlay.querySelector("#startup-status");
      if (status) {
        status.textContent = MENU_READY_STATUS;
      }
    }
  }
  function beginRound(): void {
    inRound = true;
    active = true;
    root.hidden = false;
    root.classList.remove("menu-ready");
    controls.clear();
    game.start();
    audio().start();
    canvas.focus();
    updateHud(0);
  }
  /** BATTLE SETUP: a fresh world behind the menu, prepared again before GO. */
  function restart(): void {
    controls.clear();
    game.restart();
    inRound = false;
    ui.refresh();
    updateHud(0);
  }
  const ui = new UI(root, choices, {
    start: () => void startFromMenu(),
    resume() {
      controls.clear();
      game.resume();
      updateHud(0);
    },
    restart() {
      restart();
      void preloadFromMenu();
    },
    endBattle() {
      game.end_battle();
      updateHud(0);
    },
    pause,
    setting: settings,
    touchMode: () => touchControls.preference,
    setTouchMode: (mode) => touchControls.setPreference(mode),
    selectAmmo(weapon) {
      if (phase === "playing" && alive) {
        controls.ammoSelection = weapon;
      }
    },
  });
  const toggleView = () => {
    firstPerson = game.toggle_first_person();
    holdPointer();
    controls.capturePointer();
  };
  controls.toggleView = toggleView;
  const cockpit = new Cockpit(root, toggleView);
  ui.overlay.addEventListener("change", () => void preloadFromMenu());
  ui.overlay.addEventListener("click", (event) => {
    if (!(event.target instanceof Element && event.target.closest("[data-kind]"))) {
      return;
    }
    if (inRound) {
      // The respawn menu's tank cards choose the next life's tank.
      game.set_human_kind(choices.humanKind);
      prepared = { ...prepared, humanKind: choices.humanKind };
    } else {
      void preloadFromMenu();
    }
  });
  const touchControls = new TouchModeController(
    root,
    controls,
    {
      get human() {
        return { mineCooldown: hud?.human.mineCooldown ?? 0 };
      },
      get match() {
        return { phase };
      },
    },
    zoom,
  );
  let exactResolution = false;
  const resize = () =>
    exactResolution
      ? game.resize(EXACT_RESOLUTION.width, EXACT_RESOLUTION.height, 1, true)
      : game.resize(innerWidth, innerHeight, devicePixelRatio, false);
  window.addEventListener("resize", resize);
  const recorder = new FrameRecorder(game, canvas);
  const counters = { frames: 0, events: 0 };
  const input = new Float32Array(INPUT.length);
  let stopped = false;
  let last = performance.now();
  const fail = (error: unknown) => {
    stopped = true;
    console.error("The game stopped", error);
    ui.toast.textContent = "The renderer stopped. Reload the page to play again.";
    ui.toast.classList.add("visible");
    ui.toastTime = Infinity;
  };
  function loop(now: number): void {
    if (stopped) {
      return;
    }
    const frameMs = Math.max(0, now - last);
    last = Math.max(last, now);
    if (active && !document.hidden) {
      controls.takeInput(input);
      input[INPUT.zoom] = pendingZoom;
      pendingZoom = 0;
      let result: Float32Array;
      try {
        result = game.frame(now, input);
        if (counters.frames % ERROR_CHECK_EVERY_FRAMES === 0) {
          const error = game.error();
          if (error) {
            throw new Error(error);
          }
        }
      } catch (error) {
        fail(error);
        return;
      }
      counters.frames++;
      phase = PHASES[result[FRAME.phase]] ?? "ready";
      alive = result[FRAME.humanAlive] === 1;
      firstPerson = result[FRAME.firstPerson] === 1;
      holdPointer();
      if (result[FRAME.clearInput]) {
        controls.clear();
      }
      if (result[FRAME.events] > 0) {
        const batch = JSON.parse(game.drain_events()) as EventBatch;
        counters.events += batch.events.length;
        audio().play(batch);
        // A death names its killer, who may have joined since the last HUD update.
        const state =
          hud && !batch.events.some((event) => event.type === "death") ? hud : readHud();
        for (const event of batch.events) {
          ui.event(event, state);
        }
      }
      cockpit.update(
        result[FRAME.cockpit] === 1,
        result[FRAME.hullAngle],
        controls.aimWaitsForClick,
      );
      stats.frame(now, result[FRAME.simMs], result[FRAME.renderMs]);
      if (result[FRAME.hudDue]) {
        updateHud(result[FRAME.dt] * HUD_UPDATE_EVERY_FRAMES);
      }
      recorder.capture(now, frameMs, result[FRAME.simMs], result[FRAME.renderMs]);
    }
    requestAnimationFrame(loop);
  }
  requestAnimationFrame(loop);
  if (import.meta.env.DEV) {
    Object.assign(window, {
      sloppy: createDebug(
        game,
        audio,
        controls,
        {
          start() {
            controls.clear();
            game.restart();
            beginRound();
          },
          restart,
          resize(exact) {
            exactResolution = exact;
            resize();
          },
        },
        recorder,
        counters,
      ),
    });
    if (params.has("tweak")) {
      const { Pane } = await import("tweakpane");
      const pane = new Pane({ title: "Yard workshop" });
      const view = () =>
        (JSON.parse(game.debug_json()) as { view: Record<"zoom" | "minZoom" | "maxZoom", number> })
          .view;
      const camera = {
        get zoom() {
          return view().zoom;
        },
        set zoom(value: number) {
          game.debug_set_zoom(value);
        },
      };
      const { minZoom, maxZoom } = view();
      pane.addBinding(camera, "zoom", { min: minZoom, max: maxZoom });
    }
  }

  // Let the ready menu paint first; GO still creates audio if it arrives sooner.
  requestAnimationFrame(() => setTimeout(audio, 0));
  // The first arena prepares here, so the menu reports its shader progress.
  await prepareArena(choices, onStage);
  return {
    prepare: prepareArena,
    async start(options) {
      // Usually already prepared while the player chose; then this is instant.
      Object.assign(choices, options);
      await prepareArena(choices, onStage);
      beginRound();
    },
  };
}
