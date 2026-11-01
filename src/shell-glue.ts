// The WebGPU shell's glue as the game page loads it (`index.html`), with the address it
// was loaded from: the page's bake workers (`bake-worker.ts`) import it from there, so
// they reuse the page's download instead of a copy of their own.
export * from "./generated/shell/sloppy-tanks.js";
export { default } from "./generated/shell/sloppy-tanks.js";
export const glueUrl = import.meta.url;
