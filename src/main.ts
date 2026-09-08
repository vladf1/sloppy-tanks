import "./style.css";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "./game/simulation";
import { Presentation } from "./game/presentation";
import { Controls } from "./game/controls";
import { AudioSystem } from "./game/audio";
import { UI } from "./game/ui";
import { STEP } from "./game/data";
import { tuneSpeed } from "./game/speed-tuning";
import { idleCommand } from "./game/types";
import { loadTankSurface } from "./game/tank-surfaces";
await RAPIER.init();
await loadTankSurface();
const root = document.querySelector<HTMLDivElement>("#app")!;
root.innerHTML =
  '<canvas id="game" tabindex="0" aria-label="Sloppy Tanks 3D demolition arena"></canvas><div id="fps" aria-label="Frames per second">— FPS</div>';
const fpsDisplay = root.querySelector<HTMLElement>("#fps")!;
let fpsStart = 0, fpsFrames = 0;
document.addEventListener("visibilitychange", () => {
  fpsStart = 0;
  fpsFrames = 0;
});
const canvas = document.querySelector<HTMLCanvasElement>("#game")!,
  sim = new Simulation(Math.floor(Math.random() * 1000000)),
  view = new Presentation(canvas),
  audio = new AudioSystem();
view.reset(sim);
let autoplay = new URLSearchParams(location.search).has("autoplay"),
  overview = false,
  accumulator = 0,
  last = performance.now(),
  frameIndex = 0;
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
  (n) => (view.zoom = Math.max(23, Math.min(52, view.zoom + n))),
  () => sim.match.phase === "playing" && sim.human.alive,
);
const settings = (key: string, value: number) => {
  if (key === "tank-speed" || key === "bullet-speed") value = tuneSpeed(sim, key, value);
  localStorage.setItem("sloppy-" + key, String(value));
  if (key === "volume") audio.volume(value);
};
settings("volume", Number(localStorage.getItem("sloppy-volume") ?? ".6"));
for (const key of ["tank-speed", "bullet-speed"] as const)
  settings(key, Number(localStorage.getItem("sloppy-" + key) ?? "1"));
function start() {
  controls.clear();
  sim.reset();
  view.reset(sim);
  sim.start();
  audio.start();
  canvas.focus();
  accumulator = 0;
}
function restart() {
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
);
window.addEventListener("resize", () => view.resize());
if (autoplay) start();
interface Frame {
  frame: number;
  sim: number;
  render: number;
  calls: number;
  triangles: number;
  bodies: number;
  shots: number;
  fragments: number;
  time: number;
}
const samples: Frame[] = [];
let recording = false,
  recordStart = 0;
let autoRounds = false,
  completedRounds = 0;
function loop(now: number) {
  const raw = (now - last) / 1000,
    dt = Math.min(0.1, raw);
  last = now;
  if (!document.hidden) {
    if (sim.match.phase === "results" && autoRounds) {
      controls.clear();
      completedRounds++;
      sim.reset();
      view.reset(sim);
      sim.start();
    }
    const startSim = performance.now();
    if (sim.match.phase === "playing") {
      accumulator = Math.min(accumulator + dt, STEP * 5);
      const aim = view.aim(controls.nx, controls.ny),
        p = sim.human.alive ? sim.human.body.translation() : sim.human.previous;
      const angle = Math.atan2(aim.x - p.x, aim.z - p.z);
      let steps = 0;
      while (accumulator >= STEP && steps < 5) {
        sim.step(controls.command(angle), autoplay);
        accumulator -= STEP;
        steps++;
      }
    } else accumulator = 0;
    if (sim.match.phase !== "playing" || !sim.human.alive) controls.clear();
    const simCost = performance.now() - startSim;
    const events = sim.events.splice(0);
    for (const e of events) {
      const playerHit = (e.type === "hurt" || e.type === "death") &&
        e.owner === sim.human.id && e.team !== sim.human.team;
      view.event(e, playerHit);
      audio.event(
        e,
        sim.human.alive ? sim.human.body.translation() : sim.human.previous,
        playerHit,
        e.id === sim.human.id,
      );
      ui.event(e);
    }
    const renderStart = performance.now();
    view.render(
      sim,
      sim.match.phase === "playing" ? accumulator / STEP : 1,
      dt,
      overview,
    );
    const renderCost = performance.now() - renderStart;
    if (fpsStart === 0) fpsStart = now;
    else {
      fpsFrames++;
      if (now - fpsStart >= 500) {
        fpsDisplay.textContent = `${Math.round(fpsFrames * 1000 / (now - fpsStart))} FPS`;
        fpsStart = now;
        fpsFrames = 0;
      }
    }
    if (frameIndex++ % 4 === 0) ui.update(dt * 4);
    if (recording) {
      samples.push({
        frame: raw * 1000,
        sim: simCost,
        render: renderCost,
        calls: view.renderer.info.render.calls,
        triangles: view.renderer.info.render.triangles,
        bodies: sim.world.bodies.len(),
        shots: sim.shots.length,
        fragments: sim.fragments.length,
        time: (now - recordStart) / 1000,
      });
      if (samples.length > 90000) samples.shift();
    }
  }
  requestAnimationFrame(loop);
}
requestAnimationFrame(loop);
const percentile = (a: number[], q: number) =>
  a.slice().sort((a, b) => a - b)[
    Math.min(a.length - 1, Math.floor(a.length * q))
  ] ?? 0;
const average = (a: number[]) =>
  a.reduce((s, n) => s + n, 0) / Math.max(1, a.length);
function report() {
  const frames = samples.filter((r) => r.time > 5);
  const f = frames.map((r) => r.frame);
  return {
    date: new Date().toISOString(),
    userAgent: navigator.userAgent,
    resolution: [canvas.width, canvas.height],
    samples: frames.length,
    seconds: samples.at(-1)?.time ?? 0,
    fps: 1000 / average(f),
    frameP50: percentile(f, 0.5),
    frameP95: percentile(f, 0.95),
    frameP99: percentile(f, 0.99),
    simulationMean: average(frames.map((r) => r.sim)),
    simulationP95: percentile(
      frames.map((r) => r.sim),
      0.95,
    ),
    renderMean: average(frames.map((r) => r.render)),
    drawCalls: Math.round(average(frames.map((r) => r.calls))),
    triangles: Math.round(average(frames.map((r) => r.triangles))),
    maxBodies: Math.max(0, ...frames.map((r) => r.bodies)),
    maxProjectiles: Math.max(0, ...frames.map((r) => r.shots)),
    maxFragments: Math.max(0, ...frames.map((r) => r.fragments)),
    completedRounds,
    snapshot: sim.snapshot(),
    memory:
      (performance as Performance & { memory?: { usedJSHeapSize: number } })
        .memory?.usedJSHeapSize ?? null,
  };
}
function createDebug() {
  return {
    sim,
    view,
    audio,
    controls,
    start,
    restart,
    report,
    samples,
    autoplay(value = true) {
      autoplay = value;
      return autoplay;
    },
    overview(value = true) {
      overview = value;
    },
    record() {
      samples.length = 0;
      recording = true;
      recordStart = performance.now();
    },
    stop() {
      recording = false;
      return report();
    },
    autoRounds(value = true) {
      autoRounds = value;
      return autoRounds;
    },
    exactResolution() {
      view.resize(2560, 1440, true);
    },
    stress() {
      sim.reset(24);
      view.reset(sim);
      sim.start();
      autoplay = true;
      for (let i = 0; i < sim.maxFragments; i++)
        sim.fragment(
          sim.rng.range(-15, 15),
          sim.rng.range(-15, 15),
          0xc5a978,
          0.5,
        );
      for (let i = 0; i < 200; i++) {
        const a = (i * Math.PI * 2) / 200;
        sim.shots.push({
          id: sim.nextId++,
          x: Math.sin(a) * 3,
          z: Math.cos(a) * 3,
          vx: Math.cos(a) * 45,
          vz: Math.sin(a) * 45,
          owner: sim.tanks[i % 24].id,
          team: (i % 2) as 0 | 1,
          damage: 40,
          bounces: 4,
          life: 4,
          piercing: 0, weapon: "standard",
        });
      }
    },
    collapse() {
      for (const c of [...sim.covers])
        if (c.kind === "tower" || c.kind === "drum")
          sim.damageCover(c, 999, sim.human.id, sim.humanTeam);
    },
    async soak(seconds = 1200) {
      let resets = 0,
        steps = 0;
      const startTime = performance.now();
      for (let i = 0; i < seconds * 60; i++) {
        if (sim.match.phase !== "playing") {
          sim.reset();
          view.reset(sim);
          sim.start();
          resets++;
        }
        sim.step(idleCommand(), true);
        steps++;
        if (i % 600 === 0)
          await new Promise((resolve) => setTimeout(resolve, 0));
      }
      return {
        simulatedSeconds: steps / 60,
        wallSeconds: (performance.now() - startTime) / 1000,
        resets,
        snapshot: sim.snapshot(),
      };
    },
  };
}
declare global {
  interface Window { sloppy: ReturnType<typeof createDebug> }
}
if (import.meta.env.DEV) {
  Object.assign(window, { sloppy: createDebug() });
  if (new URLSearchParams(location.search).has("tweak")) {
    const { Pane } = await import("tweakpane");
    const pane = new Pane({ title: "Yard workshop" });
    pane.addBinding(view, "zoom", { min: 23, max: 52 });
  }
}
