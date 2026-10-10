// The labs build of the engine (`RenderLab`, `EffectsLab`): `pnpm run wasm:labs`
// writes it to `src/generated/engine-labs/`, separate from the game's engine, which
// never carries the labs. Pages load it through here so a missing build explains itself.
import type * as Labs from "../src/generated/engine-labs/engine.js";

export type LabsEngine = typeof Labs;

export async function loadLabsEngine(): Promise<LabsEngine> {
  let engine: LabsEngine;
  try {
    engine = await import("../src/generated/engine-labs/engine.js");
  } catch (error) {
    throw new Error("The labs engine is not built: run `pnpm run wasm:labs`", {
      cause: error,
    });
  }
  // The glue finds its binary beside itself.
  await engine.default();
  return engine;
}

/** Compile every pipeline `budget` at a time, reporting what remains, wait for the
 * textures, then warm up. */
export async function prepareLab(
  lab: Labs.RenderLab | Labs.EffectsLab,
  budget: number,
  progress?: (remaining: number) => void,
): Promise<void> {
  for (;;) {
    const [compiled, remaining, compiling] = lab.prepare_step(budget);
    progress?.(remaining);
    if (remaining === 0) break;
    if (compiled === 0 && compiling === 0) throw new Error("Pipeline preparation made no progress");
    // Background compiles finish on their own; poll them on a short timer.
    await new Promise((resolve) => setTimeout(resolve, compiled === 0 ? 16 : 0));
  }
  // Textures upload while preparing; one still loading after the last pipeline
  // compiled needs more steps.
  for (;;) {
    lab.prepare_step(0);
    const error = lab.error();
    if (error) throw new Error(error);
    if (lab.textures_pending() === 0) break;
    await new Promise((resolve) => setTimeout(resolve, 16));
  }
  lab.warm_up();
}

/** Mark a lab page failed and show why in its `#status` line. */
export function fail(error: unknown) {
  document.body.dataset.state = "error";
  document.querySelector<HTMLElement>("#status")!.textContent = String(
    error instanceof Error ? error.message : error,
  );
  console.error(error);
}
