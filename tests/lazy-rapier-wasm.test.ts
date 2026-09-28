import assert from "node:assert/strict";
import test from "node:test";
import { lazyRapierWasm } from "../scripts/lazy-rapier-wasm";

test("SIMD bindings load without eagerly importing the WASM binary", async () => {
  const load = lazyRapierWasm().load;
  assert.equal(typeof load, "function");
  // This hook only inspects the module id and does not use a Vite plugin context.
  const run = load as (id: string) => unknown;
  for (const id of [
    "/node_modules/@dimforge/rapier3d-simd/rapier_wasm3d.js",
    "/node_modules/.pnpm/@dimforge+rapier3d-simd@0.20.0/node_modules/@dimforge/rapier3d-simd/rapier_wasm3d.js?import",
    "C:\\node_modules\\@dimforge\\rapier3d-simd\\rapier_wasm3d.js",
  ]) {
    assert.equal(await run(id), 'export * from "./rapier_wasm3d_bg.js";');
  }
  assert.equal(await run("/node_modules/@dimforge/rapier3d-simd/rapier_wasm3d_bg.js"), null);
  assert.equal(await run("/node_modules/@dimforge/rapier3d-simd/rapier_wasm3d_bg.wasm?url"), null);
});
