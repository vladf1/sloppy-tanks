import { STEP, VEHICLES } from "./data";
import type { Simulation } from "./simulation";

type SpeedSetting = keyof Simulation["speedTuning"];
/** Local playtest controls belong to this world; multiplayer uses the checked-in defaults. */
export function tuneSpeed(simulation: Simulation, key: SpeedSetting, value: number): number {
  const scale = simulation.multiplayer
    ? 1
    : Number.isFinite(value)
      ? Math.max(0.5, Math.min(2, value))
      : 1;
  const previous = simulation.speedTuning[key];
  simulation.speedTuning[key] = scale;
  if (key === "tank-speed") {
    for (const tank of simulation.tanks) {
      if (tank.alive) {
        tank.body.setSoftCcdPrediction(VEHICLES[tank.kind].speed * scale * 1.5 * STEP * 2);
      }
    }
  } else {
    for (const shot of simulation.shots) {
      shot.vx *= scale / previous;
      shot.vz *= scale / previous;
    }
  }
  return scale;
}
