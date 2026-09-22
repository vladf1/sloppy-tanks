import { sameGameOptions, type GameOptions } from "./game/game-options";
import type { StartGame } from "./game/start-menu";
import RAPIER from "@dimforge/rapier3d-compat";
import { FrameRecorder, createDebug } from "./diagnostics";
import { AudioSystem } from "./game/audio";
import { TouchModeController } from "./game/touch-mode";
import { Controls } from "./game/controls";
import { STEP } from "./game/data";
import { Presentation } from "./game/presentation";
import { Simulation } from "./game/simulation";
import { tuneSpeed } from "./game/speed-tuning";
import { loadTankSurface } from "./game/tank-surfaces";
import { UI } from "./game/ui";
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
): Promise<StartGame> {
  onStage("Building the arena…");
  const root = document.createElement("div");
  root.hidden = true;
  app.append(root);
  root.innerHTML =
    '<canvas id="game" tabindex="0" aria-label="Sloppy Tanks 3D demolition arena"></canvas>';
  const canvas = document.querySelector<HTMLCanvasElement>("#game")!;
  let view: Presentation;
  try {
    // Device setup and image decoding do not depend on the physics world.
    const results = await Promise.allSettled([
      Presentation.create(canvas),
      RAPIER.init(),
      loadTankSurface(),
    ]);
    const presentation = results[0];
    const failure = results.find((result) => result.status === "rejected");
    if (failure?.status === "rejected") {
      if (presentation.status === "fulfilled") {
        presentation.value.renderer.dispose();
      }
      throw failure.reason;
    }
    if (presentation.status !== "fulfilled") {
      throw new Error("Graphics initialization failed");
    }
    view = presentation.value;
  } catch (error) {
    root.remove();
    throw error;
  }
  const sim = new Simulation(seed);
  const preparedOptions = { ...getOptions() };
  Object.assign(sim, preparedOptions);
  const stressTest = document.documentElement.dataset.scenario === "stress-test";
  if (stressTest) {
    const { configureStressTest } = await import("./stress-test-level");
    configureStressTest(sim);
  } else {
    sim.reset();
  }
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  const audio = new AudioSystem();
  view.reset(sim);
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  onStage("Preparing graphics…");
  // Compile shaders while the menu is visible, without drawing a background scene.
  await view.prepare(sim);
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
  const settings = (key: string, value: number) => {
    if (key === "tank-speed" || key === "bullet-speed") {
      value = tuneSpeed(sim, key, value);
    }
    localStorage.setItem("sloppy-" + key, String(value));
    if (key === "volume") {
      audio.volume(value);
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
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
      let selection: GameOptions;
      do {
        selection = {
          humanKind: sim.humanKind,
          humanTeam: sim.humanTeam,
          gameMode: sim.gameMode,
          mapMode: sim.mapMode,
          difficulty: sim.difficulty,
        };
        sim.reset();
        view.reset(sim);
        await view.prepare(sim);
      } while (!sameGameOptions(selection, sim));
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
  function beginRound(): void {
    active = true;
    root.hidden = false;
    root.classList.remove("menu-ready");
    sim.start();
    audio.start();
    canvas.focus();
    accumulator = 0;
    last = performance.now();
    ui.update(0);
  }
  function restart(): void {
    controls.clear();
    sim.reset();
    view.reset(sim);
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
    restart,
    settings,
    pause,
    (weapon) => {
      if (sim.match.phase === "playing" && sim.human.alive) {
        controls.ammoSelection = weapon;
      }
    },
    (event) => view.damageAngle(event),
  );
  const touchControls = new TouchModeController(root, controls, sim, zoom);
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
        while (accumulator >= STEP && steps < MAX_CATCH_UP_STEPS) {
          sim.step(controls.command(angle), playback.autoplay);
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
        audio.event(
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
      sloppy: createDebug(sim, view, audio, controls, start, restart, recorder, playback),
    });
    if (new URLSearchParams(location.search).has("tweak")) {
      const { Pane } = await import("tweakpane");
      const pane = new Pane({ title: "Yard workshop" });
      pane.addBinding(view, "zoom", { min: CAMERA.minZoom, max: CAMERA.maxZoom });
    }
  }

  return async (options) => {
    if (!stressTest && !sameGameOptions(preparedOptions, options)) {
      // Choices can change while the earlier arena is preparing. Keep the menu
      // visible until the most recent selection, including its shaders, is ready.
      do {
        Object.assign(preparedOptions, options);
        Object.assign(sim, preparedOptions);
        sim.reset();
        view.reset(sim);
        onStage("Preparing your arena…");
        await view.prepare(sim);
      } while (!sameGameOptions(preparedOptions, options));
    }
    beginRound();
  };
}
