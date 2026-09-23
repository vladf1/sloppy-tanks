import * as RAPIER from "@dimforge/rapier3d";
export * from "@dimforge/rapier3d";
// The pinned wasm-bindgen loader installs exports at module initialization.
export default { ...RAPIER, init: async (): Promise<void> => {} };
