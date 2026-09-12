import { KMH_PER_METRE_PER_SECOND, STEP, VEHICLES, WEAPONS } from "./data";
import type { Simulation } from "./simulation";

export const speedTuning = { "tank-speed": 1, "bullet-speed": 1 };
type SpeedSetting = keyof typeof speedTuning;
const tankBases = Object.values(VEHICLES).map((v) => v.speed);
const bulletBases = Object.values(WEAPONS).map((w) => w.speed);

/** Temporary playtest controls, relative to the checked-in base speeds. */
export function tuneSpeed(simulation: Simulation, key: SpeedSetting, value: number): number {
  const scale = Number.isFinite(value) ? Math.max(0.5, Math.min(2, value)) : 1;
  const previous = speedTuning[key];
  speedTuning[key] = scale;
  if (key === "tank-speed") {
    Object.values(VEHICLES).forEach((v, i) => {
      v.speed = tankBases[i] * scale;
      v.speedKmh = Math.round(v.speed * KMH_PER_METRE_PER_SECOND);
    });
    for (const tank of simulation.tanks) {
      if (tank.alive) {
        tank.body.setSoftCcdPrediction(VEHICLES[tank.kind].speed * 1.5 * STEP * 2);
      }
    }
  } else {
    Object.values(WEAPONS).forEach((w, i) => {
      w.speed = bulletBases[i] * scale;
    });
    for (const shot of simulation.shots) {
      shot.vx *= scale / previous;
      shot.vz *= scale / previous;
    }
  }
  return scale;
}
