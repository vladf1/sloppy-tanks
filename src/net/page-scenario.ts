import { SCENARIOS, type Scenario } from "./scene-codec";

/** A scenario page (superstress.html) lists, creates and joins only its own kind of room;
 * other pages see standard rooms. */
export function pageScenario(): Scenario | undefined {
  const page = document.documentElement.dataset.scenario;
  return SCENARIOS.find((scenario) => scenario === page);
}
