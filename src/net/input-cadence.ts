import type { ControlInput } from "./player-controls";

const ACTIVE_INTERVAL_MS = 50;
const IDLE_INTERVAL_MS = 1000;
const AIM_POSITION_EPSILON = 0.01;
const AIM_ANGLE_EPSILON = 0.001;
type InputSample = Omit<ControlInput, "seq" | "observedTick">;

/** Idle refreshes retain the seat without renewing held controls at a slower rate. */
export class InputCadence {
  private previous?: InputSample;
  private sentAt = -Infinity;

  due(input: InputSample, nowMs: number): boolean {
    const elapsed = nowMs - this.sentAt;
    if (elapsed < ACTIVE_INTERVAL_MS) {
      return false;
    }
    const previous = this.previous;
    if (
      !previous ||
      input.controlEpoch !== previous.controlEpoch ||
      input.moveX !== 0 ||
      input.moveZ !== 0 ||
      input.fire ||
      input.actions.length ||
      input.moveX !== previous.moveX ||
      input.moveZ !== previous.moveZ ||
      input.fire !== previous.fire ||
      elapsed >= IDLE_INTERVAL_MS
    ) {
      return true;
    }
    const aim = input.aim;
    const oldAim = previous.aim;
    if ("angle" in aim && "angle" in oldAim) {
      const difference = aim.angle - oldAim.angle;
      return Math.abs(Math.atan2(Math.sin(difference), Math.cos(difference))) >= AIM_ANGLE_EPSILON;
    }
    if ("x" in aim && "x" in oldAim) {
      return Math.hypot(aim.x - oldAim.x, aim.z - oldAim.z) >= AIM_POSITION_EPSILON;
    }
    return true;
  }

  sent(input: InputSample, nowMs: number): void {
    this.previous = { ...input, aim: { ...input.aim }, actions: [] };
    this.sentAt = nowMs;
  }
}
