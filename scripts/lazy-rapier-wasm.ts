import type { Plugin } from "vite";

const bindings = /[\\/]@dimforge[\\/]rapier3d[\\/]rapier_wasm3d\.js$/;

/** Rapier's wasm-bindgen entry imports its binary at module scope. Bundled, that
 * becomes a top-level await, so no game module could run until the whole binary
 * arrived. Export only the JS bindings; physics-browser.ts instantiates on init(). */
export function lazyRapierWasm(): Plugin {
  return {
    name: "lazy-rapier-wasm",
    enforce: "pre",
    load(id) {
      return bindings.test(id.split("?")[0]) ? 'export * from "./rapier_wasm3d_bg.js";' : null;
    },
  };
}
