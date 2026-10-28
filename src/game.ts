// Single player in the page: the Rust engine (`crates/web/src/game.rs`) owns the
// simulation, its fixed-step loop, presentation and rendering. This module keeps the
// browser side: preparing the arena behind Battle Setup, one engine frame per
// animation frame with the packed raw input, and engine events and HUD state for
// sound and the DOM.
import {
  createGame,
  engineModule,
  loadEngine,
  mapTextureBakes,
  throwGpuError,
  type Game,
} from "./engine";
import { FrameRecorder, createDebug } from "./diagnostics";
import { printDebugHelp } from "./game/debug-console";
import { AudioSystem } from "./game/audio";
import { Cockpit } from "./game/cockpit";
import { Controls } from "./game/controls";
import {
  FRAME,
  INPUT,
  PHASES,
  type EventBatch,
  type HudState,
  type PauseReport,
  type Phase,
  type StatsSection,
} from "./game/engine-api";
import { sameGameOptions, type GameOptions } from "./game/game-options";
import {
  saveCameraPreferences,
  savedGraphics,
  savedPreference,
  savePreference,
  startingCamera,
  startingGraphics,
} from "./game/player-preferences";
import { debugPage, isExtraLevel } from "./game/map-options";
import { NerdStats } from "./game/nerd-stats";
import { savedVolume } from "./game/settings-dialog";
import type { PreparedGame } from "./game/start-menu";
import { PREPARE_BUDGET, afterPaint, nextPrepareStep, stepBetweenTasks } from "./game/task-yield";
import { bakeAhead, startTextureBake } from "./game/texture-bake";
import { TouchModeController } from "./game/touch-mode";
import { MENU_READY_STATUS, UI } from "./game/ui";
/** `window.sloppy.exactResolution()` renders at this size whatever the window. */
const EXACT_RESOLUTION = { width: 2560, height: 1440 } as const;

/** Load the engine and start the texture bakes the arena of `map` will claim, so they
 * run in workers while the engine builds the arena (`bakeAhead`). Battle Setup calls
 * this as its first frame paints and builds the arena once it settles, when the bake
 * workers have loaded; the quarry soil's bake then finishes before the arena's
 * pipelines do. */
export async function loadArenaEngine(map: string): Promise<void> {
  await loadEngine();
  await bakeAhead(engineModule(), mapTextureBakes(map));
}

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
    await loadArenaEngine(choices.mapMode);
    game = await createGame(canvas, {
      seed,
      assetBase: import.meta.env.BASE_URL,
      map: choices.mapMode,
      extraLevels: debugPage(location.search) || isExtraLevel(choices.mapMode),
      difficulty: choices.difficulty,
      humanKind: choices.humanKind,
      humanTeam: choices.humanTeam,
      gameMode: choices.gameMode,
      autoplay: params.has("autoplay"),
      cssWidth: innerWidth,
      cssHeight: innerHeight,
      pixelRatio: devicePixelRatio,
      ...startingCamera(),
      ...startingGraphics(),
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
    for (;;) {
      if (inRound) {
        return;
      }
      if (!sameGameOptions(prepared, wanted)) {
        const next = { ...wanted };
        // The rebuild blocks the main thread for up to a few hundred milliseconds; show
        // the player's new choice, or GO's LOADING…, before it starts.
        await afterPaint();
        if (inRound) {
          return;
        }
        if (!sameGameOptions(next, wanted)) {
          continue;
        }
        // Its texture bakes run in workers while the rebuild holds the main thread.
        void bakeAhead(engineModule(), mapTextureBakes(next.mapMode));
        game.set_options(JSON.stringify(next));
        prepared = next;
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
        const [, , , done, waitingForGpu] = game.prepare_step(PREPARE_BUDGET);
        gpuPending = waitingForGpu === 1;
        if (done) {
          return;
        }
        // None while a rebuild that reuses every pipeline and texture waits for its
        // warm-up frames: that is not loading, so the menu keeps showing it is ready.
        const status = game.loading_status();
        if (status) {
          reportShaders(status);
        }
      }
    }
  }

  let active = false;
  let phase: Phase = "ready";
  let alive = true;
  let hud: HudState | undefined;
  let hudDt = 0;
  const readHud = () => (hud = JSON.parse(game.hud_json()) as HudState);
  const stats = debugPage(location.search)
    ? new NerdStats(
        root,
        () => JSON.parse(game.nerd_stats_json()) as StatsSection[],
        () => active && !root.classList.contains("menu-ready"),
      )
    : undefined;
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
  const settings = (key: "volume" | "tank-speed" | "bullet-speed", value: number) => {
    if (key === "tank-speed" || key === "bullet-speed") {
      value = game.set_speed(key, value);
    }
    savePreference(key, String(value));
    if (key === "volume") {
      volume = value;
      audioSystem?.volume(value);
    }
  };
  settings("volume", savedVolume());
  for (const key of ["tank-speed", "bullet-speed"] as const) {
    settings(key, Number(savedPreference(key) ?? 1));
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
      button.textContent = "LOADING…";
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
      await revealBattle(() => {});
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
  /** Take the in-game menu away and show the battle (`showBattle`) before its round
   * starts. `hideMenu` takes away a menu that is not part of the arena's page. */
  async function revealBattle(hideMenu: () => void): Promise<void> {
    hideMenu();
    await showBattle();
  }
  /** Show the battle's view, still, and let the browser paint it before the round's
   * clock starts: the arena as `finish_prepare` drew it, the HUD, the respawn card
   * (`UI.prepareBattle`) and the touch controls. Painted the first time on the round's
   * first frame, they held WebKit's next frame for 50-70 ms, a hitch once tanks move;
   * hidden behind a menu, WebKit leaves them unpainted. Under the start menu, which is
   * opaque, they paint unseen. */
  function showBattle(): Promise<void> {
    ui.prepareBattle();
    ui.overlay.style.display = "none";
    root.classList.remove("menu-ready");
    root.hidden = false;
    // A frame starts only once the one before it reached the screen, so the second
    // waits out that paint.
    return afterPaint().then(afterPaint);
  }
  function beginRound(): void {
    inRound = true;
    active = true;
    root.hidden = false;
    root.classList.remove("menu-ready");
    controls.clear();
    hudDt = 0;
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
    graphics: savedGraphics,
    setGraphics(setting) {
      savePreference("graphics", setting);
      game.set_graphics(setting);
    },
    selectAmmo(weapon) {
      if (phase === "playing" && alive) {
        controls.ammoSelection = weapon;
      }
    },
    pauseReport: () => JSON.parse(game.pause_json()) as PauseReport,
  });
  // The damage ring wraps the tank on screen.
  const viewer = { x: NaN, y: NaN };
  ui.viewerPoint = () => (Number.isFinite(viewer.x) ? viewer : null);
  /** Switches the camera seat; the HUD's damage ring leaves the tank's screen spot at once. */
  const switchSeat = () => {
    firstPerson = game.toggle_first_person();
    if (firstPerson) {
      viewer.x = viewer.y = NaN;
    }
    ui.replaceDamageRing();
    // The cockpit frame and the lit view button follow at once, not a frame later.
    cockpit.update(firstPerson && alive, NaN, controls.aimWaitsForClick);
    return firstPerson;
  };
  const toggleView = () => {
    switchSeat();
    saveCameraPreferences(game);
    holdPointer();
    controls.capturePointer();
  };
  controls.toggleView = toggleView;
  const cockpit = new Cockpit(root, toggleView);
  ui.overlay.addEventListener("change", () => void preloadFromMenu());
  const pickTank = (event: Event) => {
    if (!(event.target instanceof Element && event.target.closest("[data-kind]"))) {
      return;
    }
    if (inRound) {
      // The respawn card's tank cards choose the next life's tank.
      game.set_human_kind(choices.humanKind);
      prepared = { ...prepared, humanKind: choices.humanKind };
    } else {
      void preloadFromMenu();
    }
  };
  ui.overlay.addEventListener("click", pickTank);
  ui.respawnLayer.addEventListener("click", pickTank);
  const touchControls = new TouchModeController(
    root,
    controls,
    () => hud?.human.mineCooldown ?? 0,
    zoom,
  );
  let exactResolution = false;
  // Render at the canvas's displayed size, which can differ from the window's (mobile
  // toolbars, a rotation still settling), so the picture is never stretched.
  const resize = () =>
    exactResolution
      ? game.resize(EXACT_RESOLUTION.width, EXACT_RESOLUTION.height, 1, true)
      : game.resize(
          canvas.clientWidth || innerWidth,
          canvas.clientHeight || innerHeight,
          devicePixelRatio,
          false,
        );
  window.addEventListener("resize", resize);
  new ResizeObserver(resize).observe(canvas);
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
    // Since the last frame the engine ran: it skips frames while the GPU catches up.
    const frameMs = Math.max(0, now - last);
    if (active && !document.hidden) {
      controls.stickTurns = touchControls.active && firstPerson;
      controls.takeInput(input);
      input[INPUT.zoom] = pendingZoom;
      pendingZoom = 0;
      let result: Float32Array;
      try {
        result = game.frame(now, input);
        if (input[INPUT.zoom] !== 0 || input[INPUT.toggleView] !== 0) {
          saveCameraPreferences(game);
        }
        throwGpuError(game, counters.frames);
      } catch (error) {
        fail(error);
        return;
      }
      const ran = result[FRAME.ran] === 1;
      if (ran) {
        counters.frames++;
        last = Math.max(last, now);
      }
      phase = PHASES[result[FRAME.phase]] ?? "ready";
      alive = result[FRAME.humanAlive] === 1;
      firstPerson = result[FRAME.firstPerson] === 1;
      viewer.x = result[FRAME.viewerX];
      viewer.y = result[FRAME.viewerY];
      holdPointer();
      if (result[FRAME.clearInput]) {
        controls.clear();
      }
      if (result[FRAME.events] > 0) {
        const batch = JSON.parse(game.drain_events()) as EventBatch;
        counters.events += batch.events.length;
        audio().play(batch);
        // The engine names a death's killer itself, so the last HUD serves.
        const state = hud ?? readHud();
        for (const event of batch.events) {
          ui.event(event, state);
        }
      }
      cockpit.update(
        result[FRAME.cockpit] === 1,
        result[FRAME.hullAngle],
        controls.aimWaitsForClick,
      );
      if (ran) {
        stats?.frame(now);
        recorder.capture(now, frameMs, result[FRAME.simMs], result[FRAME.renderMs]);
      }
      hudDt += result[FRAME.dt];
      if (result[FRAME.hudDue]) {
        updateHud(hudDt);
        hudDt = 0;
      }
    } else {
      last = Math.max(last, now);
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
          switchSeat,
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
  if (debugPage(location.search)) {
    printDebugHelp("single player");
  }

  // Let the ready menu paint first; GO still creates audio if it arrives sooner.
  requestAnimationFrame(() => setTimeout(audio, 0));
  // The first arena prepares here, so the menu reports its shader progress.
  await prepareArena(choices, onStage);
  // Once the ready menu has painted, what the arena left for later (the wrecks of the
  // vehicle library) builds, and the first round's view shows and paints under the
  // menu while the player chooses, so GO only takes the menu away: two paints after
  // GO had put the round's first frame 30-40 ms later.
  const battleShown = afterPaint().then(() => {
    void stepBetweenTasks(() => game.prebuild_step());
    return showBattle();
  });
  return {
    prepare: prepareArena,
    async start(options, hideMenu) {
      // Usually already prepared while the player chose; then this is instant.
      Object.assign(choices, options);
      await prepareArena(choices, onStage);
      await battleShown;
      hideMenu();
      beginRound();
    },
  };
}
