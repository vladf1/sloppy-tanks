// The WebGL shell's glue with the address it was loaded from (`shell-glue.ts`).
export * from "./generated/shell-webgl/sloppy-tanks-webgl.js";
export { default } from "./generated/shell-webgl/sloppy-tanks-webgl.js";
export const glueUrl = import.meta.url;
