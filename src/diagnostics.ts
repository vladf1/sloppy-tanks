import type { AudioSystem } from "./game/audio";
import type { Controls } from "./game/controls";
import type { Presentation } from "./game/presentation";
import type { Simulation } from "./game/simulation";
import { idleCommand } from "./game/types";
const MAX_SAMPLES = 90000;
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
const percentile = (a: number[], q: number) =>
  a.slice().sort((a, b) => a - b)[Math.min(a.length - 1, Math.floor(a.length * q))] ?? 0;
const average = (a: number[]) => a.reduce((s, n) => s + n, 0) / Math.max(1, a.length);
export class FrameRecorder {
  readonly samples: Frame[] = [];
  recording = false;
  recordStart = 0;
  completedRounds = 0;
  constructor(
    private sim: Simulation,
    private canvas: HTMLCanvasElement,
  ) {}
  record(): void {
    this.samples.length = 0;
    this.recording = true;
    this.recordStart = performance.now();
  }
  stop() {
    this.recording = false;
    return this.report();
  }
  capture(frame: Frame): void {
    if (!this.recording) {
      return;
    }
    this.samples.push(frame);
    if (this.samples.length > MAX_SAMPLES) {
      this.samples.shift();
    }
  }
  report() {
    const frames = this.samples.filter((r) => r.time > 5);
    const f = frames.map((r) => r.frame);
    return {
      date: new Date().toISOString(),
      userAgent: navigator.userAgent,
      resolution: [this.canvas.width, this.canvas.height],
      samples: frames.length,
      seconds: this.samples.at(-1)?.time ?? 0,
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
      completedRounds: this.completedRounds,
      snapshot: this.sim.snapshot(),
      memory:
        (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory
          ?.usedJSHeapSize ?? null,
    };
  }
}
export function createDebug(
  sim: Simulation,
  view: Presentation,
  audio: AudioSystem,
  controls: Controls,
  start: () => void,
  restart: () => void,
  recorder: FrameRecorder,
  playback: { autoplay: boolean; overview: boolean; autoRounds: boolean },
) {
  return {
    sim,
    view,
    audio,
    controls,
    start,
    restart,
    report: () => recorder.report(),
    samples: recorder.samples,
    autoplay(value = true): boolean {
      playback.autoplay = value;
      return playback.autoplay;
    },
    overview(value = true): void {
      playback.overview = value;
    },
    record(): void {
      recorder.record();
    },
    stop() {
      return recorder.stop();
    },
    autoRounds(value = true): boolean {
      playback.autoRounds = value;
      return playback.autoRounds;
    },
    exactResolution(): void {
      view.resize(2560, 1440, true);
    },
    stress(): void {
      sim.reset(24);
      view.reset(sim);
      sim.start();
      playback.autoplay = true;
      for (let i = 0; i < sim.maxFragments; i++) {
        sim.fragment(sim.rng.range(-15, 15), sim.rng.range(-15, 15), 0xc5a978, 0.5);
      }
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
          piercing: 0,
          weapon: "standard",
        });
      }
    },
    collapse(): void {
      for (const cover of [...sim.covers]) {
        if (cover.kind === "tower" || cover.kind === "drum") {
          sim.damageCover(cover, 999, sim.human.id, sim.humanTeam);
        }
      }
    },
    async soak(seconds = 1200) {
      let resets = 0;
      let steps = 0;
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
        if (i % 600 === 0) {
          await new Promise((resolve) => setTimeout(resolve, 0));
        }
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
  interface Window {
    sloppy: ReturnType<typeof createDebug>;
  }
}
