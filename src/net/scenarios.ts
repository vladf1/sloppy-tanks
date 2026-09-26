/** Room modes with their own arena and rules, created and joined from their own page. This
 * module imports nothing, so the inline startup script can recognise a scenario page. */
export const SCENARIOS = ["superstress"] as const;
export type Scenario = (typeof SCENARIOS)[number];

/** A scenario page (superstress.html) lists, creates and joins only its own kind of room.
 * Other pages, including the offline stress test, return undefined. */
export function pageScenario(): Scenario | undefined {
  const page = document.documentElement.dataset.scenario;
  return SCENARIOS.find((scenario) => scenario === page);
}
