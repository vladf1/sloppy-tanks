import type { RenderState } from "../game/render-state";
import type { SimEvent, Shot } from "../game/types";
import type { TimedEvent, ShotTrace } from "./replication";
import { RenderTimeline } from "./render-timeline";
const DISPLAY_DELAY_SECONDS = 0.1;
const MAX_HISTORY_ITEMS = 4096;
/** Arrival time estimates the server clock; only rendering interpolates or extrapolates poses. */
export class NetworkTimeline {
  private poses = new RenderTimeline();
  private events: TimedEvent[] = [];
  private traces: ShotTrace[] = [];
  private newestTick = 0;
  private receivedMs = 0;
  private displayTick = 0;
  reset(state: RenderState, tick: number, nowMs: number): void {
    this.newestTick = tick;
    this.receivedMs = nowMs;
    this.displayTick = tick;
    this.events = [];
    this.traces = [];
    this.poses.reset({ state: { ...state, elapsed: tick / 60 }, events: [], ack: 0 });
  }
  push(
    state: RenderState,
    tick: number,
    events: TimedEvent[],
    traces: ShotTrace[],
    nowMs: number,
  ): void {
    this.newestTick = tick;
    this.receivedMs = nowMs;
    this.poses.push({ state: { ...state, elapsed: tick / 60 }, events: [], ack: 0 });
    this.events.push(...events);
    this.traces.push(...traces);
    if (this.events.length > MAX_HISTORY_ITEMS || this.traces.length > MAX_HISTORY_ITEMS) {
      throw new Error("Display history overflow; resync required");
    }
  }
  read(nowMs: number, rttMs: number, dt: number): { state: RenderState; events: SimEvent[] } {
    const serverTime =
      this.newestTick / 60 +
      Math.min(0.15, Math.max(0, nowMs - this.receivedMs) / 1000) +
      Math.min(0.15, rttMs / 2000);
    this.displayTick = Math.max(
      this.displayTick,
      Math.min(this.newestTick, (serverTime - DISPLAY_DELAY_SECONDS) * 60),
    );
    const result = this.poses.read(this.displayTick / 60, serverTime, dt, "extrapolate", true);
    const events: SimEvent[] = [];
    while (this.events.length && this.events[0].tick <= this.displayTick) {
      events.push(this.events.shift()!.event);
    }
    const shots = new Map<number, Shot>();
    const traced = new Set(this.traces.map((trace) => trace.shot.id));
    for (const shot of result.state.shots) {
      if (!traced.has(shot.id) || this.displayTick === this.newestTick) {
        shots.set(shot.id, shot);
      }
    }
    for (const trace of this.traces) {
      if (trace.tick <= this.displayTick && this.displayTick < trace.endTick) {
        const alpha = (this.displayTick - trace.tick) / (trace.endTick - trace.tick);
        shots.set(trace.shot.id, {
          ...trace.shot,
          x: trace.shot.x + (trace.end.x - trace.shot.x) * alpha,
          z: trace.shot.z + (trace.end.z - trace.shot.z) * alpha,
        });
      }
    }
    this.traces = this.traces.filter((trace) => trace.endTick >= this.displayTick - 12);
    return { state: { ...result.state, shots: [...shots.values()] }, events };
  }
}
