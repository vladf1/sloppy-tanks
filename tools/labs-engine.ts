// The labs build of the engine (`RenderLab`, `EffectsLab`): `pnpm run wasm -- --labs`
// writes it to `src/generated/engine-labs/`, separate from the game's engine, which
// never carries the labs. Pages load it through here so a missing build explains itself.
import type * as Labs from "../src/generated/engine-labs/engine.js";

export type LabsEngine = typeof Labs;

export async function loadLabsEngine(): Promise<LabsEngine> {
  let engine: LabsEngine;
  try {
    engine = await import("../src/generated/engine-labs/engine.js");
  } catch (error) {
    throw new Error("The labs engine is not built: run `pnpm run wasm -- --labs`", {
      cause: error,
    });
  }
  // The glue finds its binary beside itself.
  await engine.default();
  return engine;
}
