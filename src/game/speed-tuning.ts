import { VEHICLES, WEAPONS, STEP } from "./data";
import type { Simulation } from "./simulation";

export const speedTuning = { "tank-speed": 1, "bullet-speed": 1 };
export type SpeedSetting = keyof typeof speedTuning;
const tankBases = Object.values(VEHICLES).map((v) => v.speed);
const bulletBases = Object.values(WEAPONS).map((w) => w.speed);

/** Temporary playtest controls, relative to the checked-in base speeds. */
export function tuneSpeed(s: Simulation, key: SpeedSetting, value: number) {
  const scale = Number.isFinite(value) ? Math.max(0.5, Math.min(2, value)) : 1;
  const previous = speedTuning[key];
  speedTuning[key] = scale;
  if (key === "tank-speed") {
    Object.values(VEHICLES).forEach((v, i) => {
      v.speed = tankBases[i] * scale;
      v.speedKmh = Math.round(v.speed * 3.6);
    });
    for (const t of s.tanks) if (t.alive)
      t.body.setSoftCcdPrediction(VEHICLES[t.kind].speed * 1.5 * STEP * 2);
  } else {
    Object.values(WEAPONS).forEach((w, i) => { w.speed = bulletBases[i] * scale; });
    for (const shot of s.shots) {
      shot.vx *= scale / previous;
      shot.vz *= scale / previous;
    }
  }
  return scale;
}
