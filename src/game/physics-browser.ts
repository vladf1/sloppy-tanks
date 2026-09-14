import * as RAPIER from "@dimforge/rapier3d";

export * from "@dimforge/rapier3d";

/** Vite awaits the separate WASM module before evaluating this browser entry. */
export function init(): Promise<void> {
  return Promise.resolve();
}

// The matching compat package remains the Node test/CLI entry; game code uses one API.
export default { ...RAPIER, init };
