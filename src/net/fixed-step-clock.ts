export const HOST_INTERVAL_MS = 50;
export const SIMULATION_STEP_MS = 1000 / 60;
export const MAX_STEPS_PER_BATCH = 6;
export const MAX_TICK_DEBT_MS = 250;
const CLOCK_EPSILON_MS = 1e-7;

/** Keeps elapsed time owed to physics; a stall never silently drops simulation ticks. */
export class FixedStepClock {
  tick = 0;
  debtMs = 0;
  private lastMs: number;

  constructor(nowMs: number) {
    this.lastMs = nowMs;
  }

  advance(nowMs: number, step: (tick: number) => void): boolean {
    if (!Number.isFinite(nowMs)) {
      throw new Error("Invalid host clock");
    }
    this.debtMs += Math.max(0, nowMs - this.lastMs);
    this.lastMs = Math.max(this.lastMs, nowMs);
    if (this.debtMs > MAX_TICK_DEBT_MS + CLOCK_EPSILON_MS) {
      return false;
    }
    let steps = 0;
    while (this.debtMs + CLOCK_EPSILON_MS >= SIMULATION_STEP_MS && steps < MAX_STEPS_PER_BATCH) {
      step(++this.tick);
      this.debtMs = Math.max(0, this.debtMs - SIMULATION_STEP_MS);
      steps++;
    }
    return true;
  }
}
