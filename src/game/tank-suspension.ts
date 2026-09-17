const MAX_PITCH = 0.035; // 2 degrees, including sudden stops and knockback.
const MAX_ROLL = 0.025; // 1.4 degrees.
const RESPONSE = 18;
const clamp = (value: number, limit: number) => Math.max(-limit, Math.min(limit, value));

class SuspensionAxis {
  angle = 0;
  private velocity = 0;

  /** Exact critically damped spring: soft settling without frame-rate-dependent wobble. */
  step(target: number, dt: number): void {
    const offset = this.angle - target;
    const impulse = this.velocity + RESPONSE * offset;
    const decay = Math.exp(-RESPONSE * dt);
    this.angle = target + (offset + impulse * dt) * decay;
    this.velocity = (this.velocity - RESPONSE * impulse * dt) * decay;
  }
}

/** Render-only hull response. Sampling simulation time avoids pulses between physics ticks. */
export class TankSuspension {
  readonly pitch = new SuspensionAxis();
  readonly roll = new SuspensionAxis();
  private sampleTime?: number;
  private vx = 0;
  private vz = 0;
  private pitchTarget = 0;
  private rollTarget = 0;

  update(vx: number, vz: number, heading: number, time: number, dt: number): void {
    const elapsed = this.sampleTime === undefined ? 0 : time - this.sampleTime;
    if (this.sampleTime === undefined || elapsed < 0 || elapsed > 0.2) {
      this.pitchTarget = this.rollTarget = 0;
      this.vx = vx;
      this.vz = vz;
      this.sampleTime = time;
    } else if (elapsed > 0) {
      const ax = (vx - this.vx) / elapsed;
      const az = (vz - this.vz) / elapsed;
      const sin = Math.sin(heading);
      const cos = Math.cos(heading);
      this.pitchTarget = clamp(-(ax * sin + az * cos) * 0.001125, MAX_PITCH);
      this.rollTarget = clamp((ax * cos - az * sin) * 0.00175, MAX_ROLL);
      this.vx = vx;
      this.vz = vz;
      this.sampleTime = time;
    }
    const frameTime = Math.max(0, Math.min(dt, 0.1));
    this.pitch.step(this.pitchTarget, frameTime);
    this.roll.step(this.rollTarget, frameTime);
  }
}
