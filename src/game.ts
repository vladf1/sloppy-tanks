import { gameChoices, sameGameOptions, type GameOptions } from "./game/game-options";
import type { PreparedGame } from "./game/start-menu";
import RAPIER from "@dimforge/rapier3d-compat";
import { FrameRecorder, createDebug } from "./diagnostics";
import { AudioSystem } from "./game/audio";
import { Cockpit } from "./game/cockpit";
import { TouchModeController } from "./game/touch-mode";
import { Controls } from "./game/controls";
import { STEP } from "./game/data";
import { Presentation } from "./game/presentation";
import { selectedMap, Simulation, type SimulationSetup } from "./game/simulation";
import { isExtraLevel, type MapId } from "./game/map-options";
import { singlePlayerRules, STANDARD_RULES } from "./game/level-rules";
import { tuneSpeed } from "./game/speed-tuning";
import { loadTankSurface } from "./game/tank-surfaces";
import { MENU_READY_STATUS, UI } from "./game/ui";
import { CAMERA } from "./game/view-settings";
import { NerdStats } from "./game/nerd-stats";
import { afterPaint } from "./game/task-yield";
const MAX_FRAME_DELTA_SECONDS = 0.1;
const MAX_CATCH_UP_STEPS = 5;
const HUD_UPDATE_EVERY_FRAMES = 4;
const MILLISECONDS_PER_SECOND = 1000;

/** A map's level rules. An extra level's code downloads only once a player chooses it. */
async function levelRules(mapMode: MapId): Promise<SimulationSetup> {
  if (!isExtraLevel(mapMode)) {
    return STANDARD_RULES;
  }
  const { EXTRA_LEVELS } = await import("./extra-levels");
  return singlePlayerRules(EXTRA_LEVELS[mapMode]);
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
  const canvas = document.querySelector<HTMLCanvasElement>("#game")!;
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
  let rules: SimulationSetup;
  try {
    rules = await levelRules(preparedOptions.mapMode);
    const map = selectedMap(preparedOptions.mapMode, rules.customMap);
    view.buildScenery(map.theme ?? map.id);
    await physics;
  } catch (error) {
    view.renderer.dispose();
    root.remove();
    throw error;
  }
  // Browser startup previously constructed round 2, then immediately discarded
  // it for round 3. Keep the round (it seeds bot names), build only that world.
  const sim = new Simulation(seed, { ...preparedOptions, ...rules, round: 3 });
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
  // Bumped when a round begins or resets the world outside a preparation.
  let arenaRevision = 0;
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
        const choices = gameChoices(wantedOptions);
        const rules = await levelRules(choices.mapMode);
        // The rebuild blocks the main thread for up to a few hundred milliseconds; show
        // the player's new choice, or GO's WAIT, before it starts.
        await afterPaint();
        if (!sameGameOptions(choices, wantedOptions)) {
          // Chosen again meanwhile, perhaps while an extra level downloaded.
          continue;
        }
        arena = "stale";
        Object.assign(preparedOptions, choices);
        Object.assign(sim, preparedOptions, rules);
        sim.reset();
        view.reset(sim);
        arena = "reset";
      }
      const revision = arenaRevision;
      await view.prepare(sim, (stage) => reportShaders(stage));
      if (revision !== arenaRevision) {
        // A round began or reset the world meanwhile; never rebuild a world in
        // use. Its arena state already says what the next preparation needs.
        return;
      }
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
  );
  // Creating the AudioContext can block the main thread for over 150 ms. Sounds
  // are first needed when a round begins, so this waits until the menu is ready.
  let audioSystem: AudioSystem | undefined;
  let volume = 0;
  const audio = () => {
    if (!audioSystem) {
      audioSystem = new AudioSystem();
      audioSystem.volume(volume);
      audioSystem.listenerRight = view.listenerRight;
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
    if (roundStarting || sim.match.phase !== "ready") {
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
    arenaRevision++;
    active = true;
    root.hidden = false;
    root.classList.remove("menu-ready");
    sim.start();
    audio().start();
    canvas.focus();
    accumulator = 0;
    last = performance.now();
    ui.update(0);
  }
  function restart(): void {
    controls.clear();
    sim.reset();
    view.reset(sim);
    Object.assign(preparedOptions, gameChoices(sim));
    arena = "reset";
    arenaRevision++;
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
  const look = view.firstPerson;
  const toggleView = () => {
    if (sim.match.phase !== "playing") {
      return;
    }
    look.toggle(sim.human.aim);
    controls.holdPointer(look.enabled);
    controls.capturePointer();
  };
  controls.toggleView = toggleView;
  const cockpit = new Cockpit(root, toggleView);
  ui.overlay.addEventListener("change", () => void preloadFromMenu());
  ui.overlay.addEventListener("click", (event) => {
    if (event.target instanceof Element && event.target.closest("[data-kind]")) {
      void preloadFromMenu();
    }
  });
  const touchControls = new TouchModeController(root, controls, sim, zoom);
  window.addEventListener("resize", () => view.resize());
  const recorder = new FrameRecorder(sim, canvas);
  function loop(now: number): void {
    // A queued RAF timestamp can precede beginRound() after a slow map rebuild.
    // Never run time backwards or extrapolate tanks beyond their physics poses.
    const raw = Math.max(0, (now - last) / MILLISECONDS_PER_SECOND);
    const dt = Math.min(MAX_FRAME_DELTA_SECONDS, raw);
    last = Math.max(last, now);
    if (active && !document.hidden) {
      if (sim.match.phase === "results" && playback.autoRounds) {
        controls.clear();
        recorder.completedRounds++;
        sim.reset();
        view.reset(sim);
        sim.start();
      }
      const startSim = performance.now();
      // Pause, results and the round menu need the cursor; a death keeps it captured.
      controls.holdPointer(look.enabled, sim.match.phase !== "playing");
      const lookPixels = controls.takeLook();
      if (sim.match.phase === "playing") {
        // Bound catch-up after stalls so one slow frame cannot spiral into more missed frames.
        accumulator = Math.min(accumulator + dt, STEP * MAX_CATCH_UP_STEPS);
        const position = sim.human.alive ? sim.human.body.translation() : sim.human.previous;
        let angle: number;
        if (look.enabled) {
          if (sim.human.alive) {
            const stick = controls.touch.pointers.aim === null ? 0 : controls.touch.aimX;
            look.turn(lookPixels, stick, dt);
          }
          angle = look.yaw;
        } else {
          const aim = controls.touch.aiming
            ? view.touchAim(position, controls.touch.aimX, controls.touch.aimY)
            : view.aim(controls.nx, controls.ny);
          angle = Math.atan2(aim.x - position.x, aim.z - position.z);
        }
        let steps = 0;
        while (accumulator >= STEP && steps < MAX_CATCH_UP_STEPS) {
          sim.step(look.steer(controls.command(angle)), playback.autoplay);
          accumulator -= STEP;
          steps++;
        }
      } else {
        accumulator = 0;
      }
      if (sim.match.phase !== "playing" || !sim.human.alive) {
        controls.clear();
      }
      const simCost = performance.now() - startSim;
      const events = sim.events.splice(0);
      for (const event of events) {
        const playerHit =
          (event.type === "hurt" || event.type === "death") &&
          event.owner === sim.human.id &&
          event.team !== sim.human.team;
        view.event(event, playerHit);
        audio().event(
          event,
          sim.human.alive ? sim.human.body.translation() : sim.human.previous,
          playerHit,
          event.id === sim.human.id,
        );
        ui.event(event);
      }
      const renderStart = performance.now();
      if (sim.match.phase !== "ready") {
        view.render(
          sim,
          sim.match.phase === "playing" ? accumulator / STEP : 1,
          dt,
          playback.overview,
        );
      }
      cockpit.update(
        sim.match.phase !== "ready" && view.seatWanted,
        look.screenAngle(sim.human.heading),
        controls.aimWaitsForClick,
      );
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
    requestAnimationFrame(loop);
  }
  requestAnimationFrame(loop);
  if (import.meta.env.DEV) {
    Object.assign(window, {
      sloppy: createDebug(sim, view, audio, controls, start, restart, recorder, playback),
    });
    if (new URLSearchParams(location.search).has("tweak")) {
      const { Pane } = await import("tweakpane");
      const pane = new Pane({ title: "Yard workshop" });
      pane.addBinding(view, "zoom", { min: CAMERA.minZoom, max: CAMERA.maxZoom });
    }
  }

  // Let the ready menu paint first; GO still creates audio if it arrives sooner.
  requestAnimationFrame(() => setTimeout(audio, 0));
  return {
    prepare: prepareArena,
    async start(options) {
      // Usually already prepared while the player chose; then this is instant.
      await prepareArena(options, onStage);
      beginRound();
    },
  };
}
