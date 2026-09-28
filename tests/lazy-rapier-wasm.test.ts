import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { test } from "node:test";
import { lazyRapierWasm } from "../scripts/lazy-rapier-wasm";

type Load = (id: string) => string | null;

// The plugin matches a package path, so a rename or a moved entry would stop matching
// without any error: the bindings would keep a top-level await on the whole binary.
test("the lazy plugin claims the installed Rapier bindings entry and nothing else", () => {
  const load = lazyRapierWasm().load as Load;
  const resolve = createRequire(import.meta.url).resolve;
  const entry = resolve("@dimforge/rapier3d-simd/rapier_wasm3d.js");
  const lazyBindings = 'export * from "./rapier_wasm3d_bg.js";';

  assert.equal(load(entry), lazyBindings);
  assert.equal(load(`${entry}?v=abc123`), lazyBindings, "a query string does not hide the entry");
  assert.equal(load(resolve("@dimforge/rapier3d-simd/rapier_wasm3d_bg.js")), null);
});
