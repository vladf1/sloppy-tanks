import type { RenderShot, RenderState } from "../game/render-state";
import type { SimEvent } from "../game/types";
import type { TimedEvent, ShotTrace } from "./replication";
import { RenderTimeline } from "./render-timeline";
import { PlayoutClock } from "./playout-clock";
import { SIMULATION_STEP_MS } from "./fixed-step-clock";
const MAX_HISTORY_ITEMS = 4096;
const MAX_LOCAL_LEAD_MS = 150;
/** Remote poses, events and traces share one delayed clock; only the local hull targets the present. */
export class NetworkTimeline {
  readonly clock = new PlayoutClock();
  private poses = new RenderTimeline();
  private events: TimedEvent[] = [];
  private traces: ShotTrace[] = [];
  private newestTick = 0;
  private displayTick = 0;
  reset(state: RenderState, tick: number, nowMs: number): void {
    this.newestTick = tick;
    this.clock.reset(tick, nowMs);
    this.displayTick = this.clock.displayMs / SIMULATION_STEP_MS;
    this.events = [];
    this.traces = [];
    this.poses.reset({ state: { ...state, elapsed: tick / 60 }, events: [], ack: 0 });
  }
  /** Records one message's arrival after its frames have been pushed. */
  arrive(nowMs: number): void {
    this.clock.arrive(this.newestTick, nowMs);
  }
  push(state: RenderState, tick: number, events: TimedEvent[], traces: ShotTrace[]): void {
    this.newestTick = tick;
    this.poses.push({ state: { ...state, elapsed: tick / 60 }, events: [], ack: 0 });
    this.events.push(...events);
    this.traces.push(...traces);
    if (this.events.length > MAX_HISTORY_ITEMS || this.traces.length > MAX_HISTORY_ITEMS) {
      throw new Error("Display history overflow; resync required");
    }
  }
  /** Received simulation still ahead of the display; negative while remote poses extrapolate. */
  get marginMs(): number {
    return (this.newestTick - this.displayTick) * SIMULATION_STEP_MS;
  }
  read(nowMs: number, rttMs: number, dt: number): { state: RenderState; events: SimEvent[] } {
    this.displayTick = this.clock.read(nowMs) / SIMULATION_STEP_MS;
    const newestMs = this.newestTick * SIMULATION_STEP_MS;
    // The local hull extrapolates toward the server's present: newest path time plus one way.
    const localTime =
      (Math.min(newestMs + MAX_LOCAL_LEAD_MS, this.clock.pathServerMs(nowMs)) +
        Math.min(MAX_LOCAL_LEAD_MS, rttMs / 2)) /
      1000;
    const result = this.poses.read(this.displayTick / 60, localTime, dt, "extrapolate", true);
    const events: SimEvent[] = [];
    while (this.events.length && this.events[0].tick <= this.displayTick) {
      events.push(this.events.shift()!.event);
    }
    const shots = new Map<number, RenderShot>();
    const traced = new Set(this.traces.map((trace) => trace.shot.id));
    for (const shot of result.state.shots) {
      if (!traced.has(shot.id) || this.displayTick >= this.newestTick) {
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
