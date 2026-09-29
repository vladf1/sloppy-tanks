import type { Game } from "./engine";
import type { AudioSystem } from "./game/audio";
import type { Controls } from "./game/controls";
import type { EngineStats, HudState } from "./game/engine-api";
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

/** Per-frame timings and counts while recording, for `window.sloppy.record/stop`. */
export class FrameRecorder {
  readonly samples: Frame[] = [];
  recording = false;
  recordStart = 0;
  constructor(
    private game: Game,
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
  /** One rendered frame; reads the engine's counters only while recording. */
  capture(now: number, frameMs: number, simMs: number, renderMs: number): void {
    if (!this.recording) {
      return;
    }
    const stats = JSON.parse(this.game.stats_json()) as EngineStats;
    this.samples.push({
      frame: frameMs,
      sim: simMs,
      render: renderMs,
      calls: stats.drawCalls,
      triangles: stats.triangles,
      bodies: stats.bodies,
      shots: stats.shots,
      fragments: stats.fragments,
      time: (now - this.recordStart) / 1000,
    });
    if (this.samples.length > MAX_SAMPLES) {
      this.samples.shift();
    }
  }
  report() {
    const frames = this.samples.filter((r) => r.time > 5);
    const f = frames.map((r) => r.frame);
    const debug = JSON.parse(this.game.debug_json()) as { completedRounds: number };
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
      completedRounds: debug.completedRounds,
      snapshot: JSON.parse(this.game.debug_snapshot()) as unknown,
      memory:
        (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory
          ?.usedJSHeapSize ?? null,
    };
  }
}

/** Simulation and view state as `debug_json` reports it. `sim` and `view` are fresh
 * copies on every read; writing to them changes nothing (use the methods). */
export interface DebugState {
  seed: number;
  phase: string;
  match: HudState["match"];
  elapsed: number;
  mapMode: string;
  mapName: string;
  gameMode: string;
  difficulty: string;
  endlessMatch: boolean;
  humanTeam: 0 | 1;
  humanKind: string;
  shotsFired: number;
  shots: number;
  fragments: number;
  bodies: number;
  autoplay: boolean;
  overview: boolean;
  completedRounds: number;
  prepared: boolean;
  human: Record<string, unknown> & { id: number; alive: boolean; x: number; z: number };
  tanks: Record<string, unknown>[];
  fragmentViews: Record<string, unknown>[];
  view: Record<string, unknown> & { zoom: number; inFirstPerson: boolean };
}

/** The development `window.sloppy`: engine state and controls for browser checks.
 * See `docs/rust-rewrite.md` ("Single-player shell") for the surface. */
export function createDebug(
  game: Game,
  audio: () => AudioSystem,
  controls: Controls,
  actions: { start(): void; restart(): void; resize(exact: boolean): void },
  recorder: FrameRecorder,
  counters: { frames: number; events: number },
) {
  const debug = () => JSON.parse(game.debug_json()) as DebugState;
  return {
    game,
    debug,
    get sim(): DebugState {
      return debug();
    },
    get view(): DebugState["view"] {
      return debug().view;
    },
    hud: () => JSON.parse(game.hud_json()) as HudState,
    stats: () => JSON.parse(game.stats_json()) as EngineStats,
    snapshot: () => JSON.parse(game.debug_snapshot()) as unknown,
    error: () => game.error() ?? null,
    get frames() {
      return counters.frames;
    },
    get events() {
      return counters.events;
    },
    get audio() {
      return audio();
    },
    controls,
    start: () => actions.start(),
    restart: () => actions.restart(),
    report: () => recorder.report(),
    samples: recorder.samples,
    autoplay: (value = true) => game.debug_set_autoplay(value),
    overview(value = true): void {
      game.debug_set_overview(value);
    },
    autoRounds: (value = true) => game.debug_set_auto_rounds(value),
    zoom: (value: number) => game.debug_set_zoom(value),
    firstPerson: () => game.toggle_first_person(),
    record(): void {
      recorder.record();
    },
    stop() {
      return recorder.stop();
    },
    exactResolution(): void {
      actions.resize(true);
    },
    stress(): void {
      game.debug_stress();
    },
    collapse(): void {
      game.debug_collapse();
    },
    giveAmmo(count = 5): void {
      game.debug_give_ammo(count);
    },
    killHuman(): void {
      game.debug_kill_human();
    },
    soak(seconds = 1200) {
      const startTime = performance.now();
      const result = JSON.parse(game.debug_soak(seconds)) as Record<string, unknown>;
      return { ...result, wallSeconds: (performance.now() - startTime) / 1000 };
    },
  };
}
declare global {
  interface Window {
    sloppy: ReturnType<typeof createDebug>;
  }
}
