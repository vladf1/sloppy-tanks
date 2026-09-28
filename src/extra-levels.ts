import type { ExtraLevelId } from "./game/map-options";
import type { SimulationSetup } from "./game/simulation";
import { STRESS_TEST_LEVEL } from "./stress-test-level";
import { SUPERSTRESS_LEVEL } from "./superstress-level";

/** Every extra level's arena, roster and rules, shared by single player and rooms. The
 * browser imports this module only once a player chooses an extra level. */
export const EXTRA_LEVELS: Record<ExtraLevelId, SimulationSetup> = {
  "stress-test": STRESS_TEST_LEVEL,
  superstress: SUPERSTRESS_LEVEL,
};
