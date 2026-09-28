import { MAX_FRAGMENTS, SIMULATION_RULES } from "./simulation-rules";
import type { SimulationSetup } from "./simulation";

/** The rules a standard map plays with. Applying them to a simulation clears whatever an
 * extra level set before, so one arena can switch between the two. */
export const STANDARD_RULES = {
  customMap: undefined,
  endlessMatch: false,
  roundCount: SIMULATION_RULES.defaultTankCount,
  humanHealthMultiplier: 1,
  powerUpDurationMultiplier: 1,
  ammoCrateMultiplier: 1,
  maxFragments: MAX_FRAGMENTS,
  afterStep: undefined,
} satisfies SimulationSetup;

/** Single player fights an extra level as one endless team battle with its whole roster. */
export function singlePlayerRules(level: SimulationSetup): SimulationSetup {
  return { ...STANDARD_RULES, ...level, gameMode: "team", endlessMatch: true };
}
