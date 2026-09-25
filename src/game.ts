import { gameChoices, sameGameOptions, type GameOptions } from "./game/game-options";
import type { PreparedGame } from "./game/start-menu";
import RAPIER from "@dimforge/rapier3d-compat";
import { FrameRecorder, createDebug } from "./diagnostics";
import { AudioSystem } from "./game/audio";
import { TouchModeController } from "./game/touch-mode";
import { Controls } from "./game/controls";
import { STEP } from "./game/data";
import { Presentation } from "./game/presentation";
import { selectedMap, Simulation, type SimulationSetup } from "./game/simulation";
import { tuneSpeed } from "./game/speed-tuning";
import { loadTankSurface } from "./game/tank-surfaces";
import { MENU_READY_STATUS, UI } from "./game/ui";
import { CAMERA } from "./game/view-settings";
import { NerdStats } from "./game/nerd-stats";
const MAX_FRAME_DELTA_SECONDS = 0.1;
const MAX_CATCH_UP_STEPS = 5;
const HUD_UPDATE_EVERY_FRAMES = 4;
const MILLISECONDS_PER_SECOND = 1000;

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
  const canvas = document.querySelector<HTMLCanvasElement>("#game")!;
  const stressTest = document.documentElement.dataset.scenario === "stress-test";
  // The physics binary is the largest download. Device setup, image decoding and
  // map scenery do not need it, so they proceed while it arrives and compiles.
  const physics = RAPIER.init();
  // Graphics setup may fail first; the await below still reports physics errors.
  physics.catch(() => {});
  const preparedOptions = { ...getOptions() };
  let view: Presentation;
  try {
    [view] = await Promise.all([Presentation.create(canvas), loadTankSurface()]);
  } catch (error) {
    root.remove();
    throw error;
  }
  let stressSetup: SimulationSetup;
  try {
    stressSetup = stressTest ? (await import("./stress-test-level")).STRESS_TEST_SETUP : {};
    const map = selectedMap(preparedOptions.mapMode, stressSetup.customMap);
    view.buildScenery(map.theme ?? map.id);
    await physics;
  } catch (error) {
    view.renderer.dispose();
    root.remove();
    throw error;
  }
  // Browser startup previously constructed round 2, then immediately discarded
  // it for round 3. Keep the round (it seeds bot names), build only that world.
  const sim = new Simulation(seed, { ...preparedOptions, ...stressSetup, round: 3 });
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  view.reset(sim);
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  onStage("Preparing graphics…");
  // Compile shaders while the menu is visible, without drawing a background scene.
  await view.prepare(sim, onStage);
  // The hidden arena GO starts. "reset" still needs its shaders and first frame;
  // "stale" has played a round and needs a new world first.
  let arena: "prepared" | "reset" | "stale" = "prepared";
  let wantedOptions: GameOptions = preparedOptions;
  let reportShaders = onStage;
  let arenaPreparation: Promise<void> | undefined;
  /** Bring the hidden arena to `options` while the player is still choosing.
   * One preparation runs at a time; choices changed meanwhile are picked up by
   * its loop, and choices that already match cost nothing. */
  function prepareArena(options: GameOptions, report: (stage: string) => void): Promise<void> {
    wantedOptions = options;
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
    while (arena !== "prepared" || !sameGameOptions(preparedOptions, wantedOptions)) {
      if (arena === "stale" || !sameGameOptions(preparedOptions, wantedOptions)) {
        arena = "stale";
        Object.assign(preparedOptions, gameChoices(wantedOptions));
        Object.assign(sim, preparedOptions);
        sim.reset();
        view.reset(sim);
        arena = "reset";
      }
      await view.prepare(sim, (stage) => reportShaders(stage));
      arena = "prepared";
    }
  }
  let active = false;
  const stats = new NerdStats(
    root,
    sim,
    view,
    () => active && !root.classList.contains("menu-ready"),
  );
  const playback = {
    autoplay: new URLSearchParams(location.search).has("autoplay"),
    overview: false,
    autoRounds: false,
  };
  let accumulator = 0;
  let last = performance.now();
  let frameIndex = 0;
  const pause = () => {
    if (sim.match.phase === "playing") {
      sim.match.phase = "paused";
      controls.clear();
      accumulator = 0;
    }
  };
  const zoom = (n: number) => {
    view.zoom = Math.max(CAMERA.minZoom, Math.min(CAMERA.maxZoom, view.zoom + n));
  };
  const controls = new Controls(
    canvas,
    pause,
    zoom,
    () => sim.match.phase === "playing" && sim.human.alive,
    !stressTest,
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
      value = tuneSpeed(sim, key, value);
    }
    localStorage.setItem("sloppy-" + key, String(value));
    if (key === "volume") {
      volume = value;
      audioSystem?.volume(value);
    }
  };
  settings("volume", Number(localStorage.getItem("sloppy-volume") ?? ".6"));
  for (const key of ["tank-speed", "bullet-speed"] as const) {
    settings(key, Number(localStorage.getItem("sloppy-" + key) ?? "1"));
  }
  function start(): void {
    controls.clear();
    sim.reset();
    view.reset(sim);
    beginRound();
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
      if (arena !== "prepared" || !sameGameOptions(preparedOptions, sim)) {
        // Let WAIT paint before the synchronous world rebuild.
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
      }
      await prepareArena(sim, (stage) => {
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
  /** The in-game battle setup edits the simulation's choices directly. */
  async function preloadFromMenu(): Promise<void> {
    if (stressTest || roundStarting || sim.match.phase !== "ready") {
      return;
    }
    try {
      await prepareArena(sim, (stage) => {
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
    if (!roundStarting && sim.match.phase === "ready" && ui.overlay.dataset.state !== "ready") {
      ui.overlay.dataset.state = "ready";
      const status = ui.overlay.querySelector("#startup-status");
      if (status) {
        status.textContent = MENU_READY_STATUS;
      }
    }
  }
  function beginRound(): void {
    arena = "stale";
    active = true;
    root.hidden = false;
    root.classList.remove("menu-ready");
    sim.start();
    audio().start();
    canvas.focus();
    accumulator = 0;
    last = performance.now();
    latency?.reset(last);
    ui.update(0);
  }
  function restart(): void {
    controls.clear();
    sim.reset();
    view.reset(sim);
    Object.assign(preparedOptions, gameChoices(sim));
    arena = "reset";
    ui.lastPhase = "";
    accumulator = 0;
  }
  const ui = new UI(
    root,
    sim,
    () => {
      void startFromMenu();
    },
    () => {
      controls.clear();
      sim.start();
      accumulator = 0;
    },
    () => {
      restart();
      void preloadFromMenu();
    },
    settings,
    pause,
    (weapon) => {
      if (sim.match.phase === "playing" && sim.human.alive) {
        controls.ammoSelection = weapon;
      }
    },
    (event) => view.damageAngle(event),
  );
  ui.overlay.addEventListener("change", () => void preloadFromMenu());
  ui.overlay.addEventListener("click", (event) => {
    if (event.target instanceof Element && event.target.closest("[data-kind]")) {
      void preloadFromMenu();
    }
  });
  const touchControls = new TouchModeController(root, controls, sim, zoom);
  const latency =
    import.meta.env.DEV && new URLSearchParams(location.search).has("latency")
      ? new (await import("./net/latency-experiment")).LatencyExperiment(
          sim,
          root,
          new URLSearchParams(location.search),
        )
      : undefined;
  window.addEventListener("resize", () => view.resize());
  const recorder = new FrameRecorder(sim, canvas);
  let frameRequest = 0;
  let backgroundTimer = 0;
  document.addEventListener("visibilitychange", () => {
    if (!stressTest) {
      return;
    }
    cancelAnimationFrame(frameRequest);
    clearTimeout(backgroundTimer);
    loop(performance.now());
  });
  function loop(now: number): void {
    // A queued RAF timestamp can precede beginRound() after a slow map rebuild.
    // Never run time backwards or extrapolate tanks beyond their physics poses.
    const raw = Math.max(0, (now - last) / MILLISECONDS_PER_SECOND);
    const dt = Math.min(MAX_FRAME_DELTA_SECONDS, raw);
    last = Math.max(last, now);
    if (active && (stressTest || !document.hidden)) {
      if (sim.match.phase === "results" && playback.autoRounds) {
        controls.clear();
        recorder.completedRounds++;
        sim.reset();
        view.reset(sim);
        sim.start();
      }
      const startSim = performance.now();
      if (sim.match.phase === "playing") {
        // Bound catch-up after stalls so one slow frame cannot spiral into more missed frames.
        accumulator = Math.min(accumulator + dt, STEP * MAX_CATCH_UP_STEPS);
        const position = sim.human.alive ? sim.human.body.translation() : sim.human.previous;
        const aim = controls.touch.aiming
          ? view.touchAim(position, controls.touch.aimX, controls.touch.aimY)
          : view.aim(controls.nx, controls.ny);
        const angle = Math.atan2(aim.x - position.x, aim.z - position.z);
        let steps = 0;
        if (latency) {
          latency.advance(now, controls.command(angle), aim, playback.autoplay);
          accumulator = 0;
        }
        while (!latency && accumulator >= STEP && steps < MAX_CATCH_UP_STEPS) {
          sim.step(controls.command(angle), playback.autoplay);
          accumulator -= STEP;
          steps++;
        }
      } else {
        latency?.pause();
        accumulator = 0;
      }
      if (sim.match.phase !== "playing" || !sim.human.alive) {
        controls.clear();
      }
      const simCost = performance.now() - startSim;
      if (latency) {
        ui.displayState = latency.state;
      }
      const events = latency ? latency.events() : sim.events.splice(0);
      for (const event of events) {
        const playerHit =
          (event.type === "hurt" || event.type === "death") &&
          event.owner === sim.human.id &&
          event.team !== sim.human.team;
        view.event(event, playerHit);
        audio().event(
          event,
          latency?.state.viewer.position ??
            (sim.human.alive ? sim.human.body.translation() : sim.human.previous),
          playerHit,
          event.id === sim.human.id,
        );
        ui.event(event);
      }
      const renderStart = performance.now();
      if (sim.match.phase !== "ready") {
        view.render(
          latency?.state ?? sim,
          !latency && sim.match.phase === "playing" ? accumulator / STEP : 1,
          dt,
          playback.overview,
        );
      }
      if (frameIndex === 0) {
        document.querySelector("#loading")?.remove();
      }
      const renderCost = performance.now() - renderStart;
      stats.frame(now, simCost, renderCost);
      if (frameIndex++ % HUD_UPDATE_EVERY_FRAMES === 0) {
        ui.update(dt * HUD_UPDATE_EVERY_FRAMES);
        touchControls.update();
      }
      if (recorder.recording) {
        recorder.capture({
          frame: raw * MILLISECONDS_PER_SECOND,
          sim: simCost,
          render: renderCost,
          calls: view.renderer.info.render.drawCalls,
          triangles: view.renderer.info.render.triangles,
          bodies: sim.world.bodies.len(),
          shots: sim.shots.length,
          fragments: sim.fragments.length,
          time: (now - recorder.recordStart) / MILLISECONDS_PER_SECOND,
        });
      }
    }
    if (stressTest && document.hidden) {
      backgroundTimer = window.setTimeout(() => loop(performance.now()), 1000 / 60);
    } else {
      frameRequest = requestAnimationFrame(loop);
    }
  }
  frameRequest = requestAnimationFrame(loop);
  if (import.meta.env.DEV) {
    Object.assign(window, {
      sloppy: Object.assign(
        createDebug(sim, view, audio, controls, start, restart, recorder, playback),
        { latency },
      ),
    });
    if (new URLSearchParams(location.search).has("tweak")) {
      const { Pane } = await import("tweakpane");
      const pane = new Pane({ title: "Yard workshop" });
      pane.addBinding(view, "zoom", { min: CAMERA.minZoom, max: CAMERA.maxZoom });
    }
  }

  // Let the ready menu paint first; GO still creates audio if it arrives sooner.
  requestAnimationFrame(() => setTimeout(audio, 0));
  // The stress level ignores menu choices, so its first arena is the only one.
  return {
    prepare: (options, onShaders) =>
      stressTest ? Promise.resolve() : prepareArena(options, onShaders),
    async start(options) {
      if (!stressTest) {
        // Usually already prepared while the player chose; then this is instant.
        await prepareArena(options, onStage);
      }
      beginRound();
    },
  };
}
