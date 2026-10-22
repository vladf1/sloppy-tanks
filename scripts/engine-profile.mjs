// Which Cargo profile built each generated engine. `pnpm run wasm:fast` (the `wasm-fast`
// profile in Cargo.toml) compiles in about half the time but optimizes less, so a bundle
// must never ship one and a measurement must never time one; scripts/build-wasm.mjs
// writes the stamp this reads beside each engine's glue.
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** The stamp file, beside the glue, holding the name of the profile that built it. */
export const PROFILE_STAMP = "build-profile.txt";

/** Every generated engine: the game's WebGPU and WebGL2 builds and the labs build. */
export const ENGINE_DIRS = [
  "src/generated/engine",
  "src/generated/engine-webgl",
  "src/generated/engine-labs",
];

/** Those of `dirs` whose engine was last built with a profile other than release. An
 * engine without a stamp predates the fast profile, so it is a release build. */
export function fastEngines(dirs = ENGINE_DIRS) {
  return dirs.filter((dir) => {
    const stamp = fileURLToPath(new URL(`../${dir}/${PROFILE_STAMP}`, import.meta.url));
    return existsSync(stamp) && readFileSync(stamp, "utf8").trim() !== "release";
  });
}
