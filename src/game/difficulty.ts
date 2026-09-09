import type { Simulation } from "./simulation";
import type { Tank } from "./types";

export const DIFFICULTIES = {
  easy: {
    label: "Easy",
    description: "Slower, less accurate enemies · 30% less enemy damage",
    reaction: 1.7,
    aimError: 1.65,
    reload: 1.35,
    damage: 0.7,
  },
  normal: {
    label: "Normal",
    description: "The original combat balance",
    reaction: 1,
    aimError: 1,
    reload: 1,
    damage: 1,
  },
  hard: {
    label: "Hard",
    description: "Faster, sharper enemies · 15% more enemy damage",
    reaction: 0.7,
    aimError: 0.65,
    reload: 0.85,
    damage: 1.15,
  },
} as const;
export type Difficulty = keyof typeof DIFFICULTIES;
export function parseDifficulty(value: string | null): Difficulty {
  return value === "easy" || value === "hard" ? value : "normal";
}
/** Allies retain the original behavior; mode-specific Solo tuning still applies. */
export function enemyDifficulty(simulation: Simulation, tank: Tank) {
  return DIFFICULTIES[
    !tank.human && tank.team !== simulation.humanTeam ? simulation.difficulty : "normal"
  ];
}
