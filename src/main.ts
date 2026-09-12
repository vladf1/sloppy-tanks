import RAPIER from "@dimforge/rapier3d-compat";
import { FrameRecorder, createDebug } from "./diagnostics";
import { AudioSystem } from "./game/audio";
import { Controls } from "./game/controls";
import { parseDifficulty } from "./game/difficulty";
import { STEP } from "./game/data";
import { Presentation } from "./game/presentation";
import { Simulation } from "./game/simulation";
import { tuneSpeed } from "./game/speed-tuning";
import { loadTankSurface } from "./game/tank-surfaces";
import { UI } from "./game/ui";
import { CAMERA } from "./game/view-settings";
import "./style.css";
const MAX_FRAME_DELTA_SECONDS = 0.1;
const MAX_CATCH_UP_STEPS = 5;
const HUD_UPDATE_EVERY_FRAMES = 4;
const FPS_UPDATE_INTERVAL_MS = 500;
const MILLISECONDS_PER_SECOND = 1000;

await RAPIER.init();
await loadTankSurface();
const root = document.querySelector<HTMLDivElement>("#app")!;
root.innerHTML =
  '<canvas id="game" tabindex="0" aria-label="Sloppy Tanks 3D demolition arena"></canvas><div id="fps" aria-label="Frames per second">— FPS</div>';
const fpsDisplay = root.querySelector<HTMLElement>("#fps")!;
let fpsStart = 0;
let fpsFrames = 0;
document.addEventListener("visibilitychange", () => {
  fpsStart = 0;
  fpsFrames = 0;
});
const canvas = document.querySelector<HTMLCanvasElement>("#game")!;
const sim = new Simulation(Math.floor(Math.random() * 1000000));
const requestedMap = new URLSearchParams(location.search).get("map");
if (requestedMap === "harbor" || requestedMap === "village" || requestedMap === "random") {
  sim.mapMode = requestedMap;
  sim.reset();
}
sim.difficulty = parseDifficulty(localStorage.getItem("sloppy-difficulty"));
const view = new Presentation(canvas);
const audio = new AudioSystem();
view.reset(sim);
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
const controls = new Controls(
  canvas,
  pause,
  (n) => (view.zoom = Math.max(CAMERA.minZoom, Math.min(CAMERA.maxZoom, view.zoom + n))),
  () => sim.match.phase === "playing" && sim.human.alive,
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
  sim.start();
  audio.start();
  canvas.focus();
  accumulator = 0;
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
  start,
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
window.addEventListener("resize", () => view.resize());
if (playback.autoplay) {
  start();
}
const recorder = new FrameRecorder(sim, canvas);
function loop(now: number): void {
  const raw = (now - last) / MILLISECONDS_PER_SECOND;
  const dt = Math.min(MAX_FRAME_DELTA_SECONDS, raw);
  last = now;
  if (!document.hidden) {
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
      const aim = view.aim(controls.nx, controls.ny);
      const position = sim.human.alive ? sim.human.body.translation() : sim.human.previous;
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
    view.render(sim, sim.match.phase === "playing" ? accumulator / STEP : 1, dt, playback.overview);
    const renderCost = performance.now() - renderStart;
    if (fpsStart === 0) {
      fpsStart = now;
    } else {
      fpsFrames++;
      if (now - fpsStart >= FPS_UPDATE_INTERVAL_MS) {
        fpsDisplay.textContent = `${Math.round((fpsFrames * MILLISECONDS_PER_SECOND) / (now - fpsStart))} FPS`;
        fpsStart = now;
        fpsFrames = 0;
      }
    }
    if (frameIndex++ % HUD_UPDATE_EVERY_FRAMES === 0) {
      ui.update(dt * HUD_UPDATE_EVERY_FRAMES);
    }
    if (recorder.recording) {
      recorder.capture({
        frame: raw * MILLISECONDS_PER_SECOND,
        sim: simCost,
        render: renderCost,
        calls: view.renderer.info.render.calls,
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
