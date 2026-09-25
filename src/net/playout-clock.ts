import { HOST_INTERVAL_MS, SIMULATION_STEP_MS } from "./fixed-step-clock";
import { MAX_EXTRAPOLATION_SECONDS } from "./render-timeline";

/** Covers tick quantization and host timer slop on top of one batch interval. */
const BUFFER_MARGIN_MS = 20;
const MIN_BUFFER_MS = HOST_INTERVAL_MS + BUFFER_MARGIN_MS;
const MAX_BUFFER_MS = 250;
const LATENESS_WINDOW_MS = 3000;
const LATENESS_PERCENTILE = 0.95;
const BUFFER_SHRINK_MS_PER_SECOND = 20;
/** A faster arrival lowers the path estimate at once; a slower route is followed gradually. */
const PATH_RISE_MS_PER_SECOND = 5;
const MAX_SLEW = 0.1;
/** Display error that produces the full slew rate. */
const SLEW_RESPONSE_MS = 1000;
const SNAP_MS = 500;
const MAX_FRAME_MS = 250;
const UNDERRUN_AVERAGE_MS = 2000;
const MAX_UNDERRUN_MS = MAX_EXTRAPOLATION_SECONDS * 1000;

interface Arrival {
  atMs: number;
  /** Local arrival time minus server simulation time. */
  pathMs: number;
}

/**
 * Maps local time to the server simulation time being displayed. Arrival jitter is absorbed by
 * an adaptive buffer behind the fastest observed path, and the display advances at a slewed
 * real-time rate instead of restarting from each packet, so a late packet never pauses motion
 * until the buffer is exhausted.
 */
export class PlayoutClock {
  /** Server simulation time being displayed, in milliseconds. */
  displayMs = 0;
  bufferMs = MIN_BUFFER_MS;
  /** Fraction of recent frames that ran past the newest snapshot. */
  underrun = 0;
  private newestMs = 0;
  private pathMs = 0;
  private targetBufferMs = MIN_BUFFER_MS;
  private arrivals: Arrival[] = [];
  private lastReadMs?: number;

  reset(tick: number, nowMs: number): void {
    this.newestMs = tick * SIMULATION_STEP_MS;
    this.pathMs = nowMs - this.newestMs;
    this.arrivals = [{ atMs: nowMs, pathMs: this.pathMs }];
    this.bufferMs = this.targetBufferMs = MIN_BUFFER_MS;
    this.displayMs = this.newestMs - this.bufferMs;
    this.lastReadMs = undefined;
    this.underrun = 0;
  }

  /**
   * Call once per received message with its newest tick. Earlier frames of the same batch were
   * simulated sooner, not delivered later, so they must not count as late arrivals.
   */
  arrive(tick: number, nowMs: number): void {
    const serverMs = tick * SIMULATION_STEP_MS;
    const sample = nowMs - serverMs;
    this.newestMs = Math.max(this.newestMs, serverMs);
    const previous = this.arrivals.at(-1);
    const elapsed = previous ? Math.max(0, nowMs - previous.atMs) : 0;
    this.pathMs = Math.min(sample, this.pathMs + (PATH_RISE_MS_PER_SECOND * elapsed) / 1000);
    this.arrivals.push({ atMs: nowMs, pathMs: sample });
    while (this.arrivals[0].atMs < nowMs - LATENESS_WINDOW_MS) {
      this.arrivals.shift();
    }
    const lateness = this.arrivals
      .map((arrival) => arrival.pathMs - this.pathMs)
      .sort((a, b) => a - b);
    const percentile = lateness[Math.floor((lateness.length - 1) * LATENESS_PERCENTILE)];
    this.targetBufferMs = Math.max(
      MIN_BUFFER_MS,
      Math.min(MAX_BUFFER_MS, MIN_BUFFER_MS + percentile),
    );
    // A stall grows the buffer immediately; recovery gives the delay back slowly in read().
    this.bufferMs = Math.max(this.bufferMs, this.targetBufferMs);
  }

  /** Server time the newest arrival would carry if it had taken the fastest recent path. */
  pathServerMs(nowMs: number): number {
    return nowMs - this.pathMs;
  }

  read(nowMs: number): number {
    const dt =
      this.lastReadMs === undefined
        ? 0
        : Math.max(0, Math.min(MAX_FRAME_MS, nowMs - this.lastReadMs));
    this.lastReadMs = nowMs;
    if (this.bufferMs > this.targetBufferMs) {
      this.bufferMs = Math.max(
        this.targetBufferMs,
        this.bufferMs - (BUFFER_SHRINK_MS_PER_SECOND * dt) / 1000,
      );
    }
    const target = nowMs - this.pathMs - this.bufferMs;
    const error = target - (this.displayMs + dt);
    if (error > SNAP_MS) {
      this.displayMs = target;
    } else {
      const slew = Math.max(-MAX_SLEW, Math.min(MAX_SLEW, error / SLEW_RESPONSE_MS));
      this.displayMs += dt * (1 + slew);
    }
    this.displayMs = Math.min(this.displayMs, this.newestMs + MAX_UNDERRUN_MS);
    const starved = this.displayMs > this.newestMs ? 1 : 0;
    this.underrun += (starved - this.underrun) * (1 - Math.exp(-dt / UNDERRUN_AVERAGE_MS));
    return this.displayMs;
  }
}
