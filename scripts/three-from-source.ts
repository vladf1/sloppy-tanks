import type { Plugin } from "vite";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const threeSource = resolve(dirname(fileURLToPath(import.meta.resolve("three"))), "../src");

const entries: Record<string, string> = {
  three: "Three.js",
  "three/webgpu": "Three.WebGPU.js",
  "three/tsl": "Three.TSL.js",
};

/** `three/webgpu` ships as one prebuilt 1.7 MB module that the bundler cannot
 * prune well. Bundle the same entry points from Three's source modules instead,
 * so unused code drops out. All three entries must come from source to share
 * one copy of the core classes. r185: review when upgrading Three. */
export function threeFromSource(): Plugin {
  return {
    name: "three-from-source",
    enforce: "pre",
    resolveId(source) {
      return source in entries ? resolve(threeSource, entries[source]) : null;
    },
  };
}
