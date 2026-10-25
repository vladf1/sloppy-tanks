// Build the browser engine: release Wasm for `sloppy-web`, then its wasm-bindgen
// glue into the ignored `src/generated/engine/`. Pages import
// `src/generated/engine/engine.js` and the binary through the `?url` asset loader, and
// initialize with `init({ module_or_path: wasmUrl })`. Bun does not compile Rust:
// rerun this after Rust or WGSL edits.
//
// A second build renders with WebGL2 instead of WebGPU (`sloppy-web`'s `webgl`
// feature) into `src/generated/engine-webgl/engine-webgl.js` and `engine-webgl_bg.wasm`;
// src/engine.ts loads it only where WebGPU is unavailable. Its own file names keep its
// binary apart from the WebGPU one in the bundle. Rebuilding it doubled the compile work
// of every edit while agents play WebGPU, so by default only the WebGPU engine is built,
// and the WebGL one too only when its glue is missing (Bun resolves its import) or its
// API no longer matches (the checker checks both against the page). `--webgl` builds the WebGL
// engine alone, for `?webgl` and the browser checks that play it; `--all` builds both,
// as `bun run build` does. Two builds run at once, the WebGL one in its own target
// directory (`target/webgl`) so neither waits on the other's Cargo lock.
//
// Cargo runs from the repository root so `.cargo/config.toml` (SIMD) applies.
// Setup: rustup's wasm32-unknown-unknown target and `wasm-bindgen-cli` 0.2.129,
// matching the `wasm-bindgen` crate pin.
//
// Like the server build, the Wasm is stamped with the content version clients and the
// server must share (`SLOPPY_CONTENT_VERSION`): multiplayer joins send it. The same stamp
// goes to `src/generated/engine/content-version.js` for Battle Setup's room list, which
// runs before the engine loads.
//
// `--labs` adds the development labs (`RenderLab`, `EffectsLab`, the `labs` feature of
// `sloppy-web`) and writes their glue to `src/generated/engine-labs/` instead, so a lab
// build never replaces the production engine that the game, `bun run build` and the dev
// server load.
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { contentVersion } from "./content-version.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const labs = process.argv.includes("--labs");
const all = process.argv.includes("--all");
const webglOnly = process.argv.includes("--webgl");
const version = await contentVersion();

const WEBGPU = {
  features: [],
  targetDir: "target",
  dir: "src/generated/engine",
  outName: "engine",
};
const WEBGL = {
  features: ["--no-default-features", "--features", "webgl"],
  targetDir: "target/webgl",
  dir: "src/generated/engine-webgl",
  outName: "engine-webgl",
};
const LABS = {
  features: ["--features", "labs"],
  targetDir: "target",
  dir: "src/generated/engine-labs",
  outName: "engine-labs",
};

/** Run `command` from the repository root; rejects unless it succeeds. */
function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: root,
      stdio: "inherit",
      env: { ...process.env, SLOPPY_CONTENT_VERSION: version },
    });
    child.on("error", (error) =>
      reject(
        new Error(
          `${command}: ${error.message}. Install Rust with the wasm32-unknown-unknown target and wasm-bindgen-cli 0.2.129.`,
        ),
      ),
    );
    child.on("exit", (status) =>
      status === 0 ? resolve() : reject(new Error(`${command} exited with ${status}`)),
    );
  });
}

/** Build `sloppy-web` with `features` in `targetDir`, then write its glue as `outName`
 * in `dir`. */
async function buildEngine({ features, targetDir, dir, outName }) {
  await run("cargo", [
    "build",
    "--locked",
    "--release",
    "--target",
    "wasm32-unknown-unknown",
    "--target-dir",
    targetDir,
    "-p",
    "sloppy-web",
    ...features,
  ]);
  await run("wasm-bindgen", [
    `${targetDir}/wasm32-unknown-unknown/release/sloppy_web.wasm`,
    "--target",
    "web",
    "--out-dir",
    dir,
    "--out-name",
    outName,
  ]);
}

/** Build `engines` at once; exits the process if any fails. */
async function buildEngines(engines) {
  // Cargo creates a missing target directory under a temporary name and renames it into
  // place, so on a fresh checkout the outer build could replace `target/` while the
  // inner one works in it. Both exist before either build starts.
  for (const { targetDir } of engines) {
    await mkdir(fileURLToPath(new URL(`../${targetDir}`, import.meta.url)), { recursive: true });
  }
  const results = await Promise.allSettled(engines.map(buildEngine));
  const failure = results.find((result) => result.status === "rejected");
  if (failure) {
    console.error(failure.reason.message);
    process.exit(1);
  }
}

/** The page-facing declarations of an engine's glue (its classes and functions), without
 * the Wasm exports that follow them and differ between builds; undefined until the glue
 * and binary exist. */
async function publicApi({ dir, outName }) {
  const path = (suffix) => fileURLToPath(new URL(`../${dir}/${outName}${suffix}`, import.meta.url));
  if (![".js", "_bg.wasm", ".d.ts"].every((suffix) => existsSync(path(suffix)))) {
    return undefined;
  }
  const types = await readFile(path(".d.ts"), "utf8");
  return types.split("export type InitInput")[0];
}

/** The engines `--labs`, `--webgl` or `--all` ask for; by default the WebGPU one, with
 * the WebGL one beside it while its glue is missing. */
async function requestedEngines() {
  if (labs) return [LABS];
  if (webglOnly) return [WEBGL];
  if (all || (await publicApi(WEBGL)) === undefined) return [WEBGPU, WEBGL];
  return [WEBGPU];
}

const engines = await requestedEngines();
await buildEngines(engines);
// The page and checker treat both glues as one API, so a WebGPU build that changed it
// rebuilds the WebGL engine as well.
if (engines.length === 1 && engines[0] === WEBGPU) {
  if ((await publicApi(WEBGPU)) !== (await publicApi(WEBGL))) {
    console.log("The WebGL engine's API no longer matches the WebGPU engine's: building it too.");
    await buildEngines([WEBGL]);
  }
}
if (!webglOnly) {
  const stamp = fileURLToPath(new URL(`../${engines[0].dir}/content-version`, import.meta.url));
  await writeFile(
    `${stamp}.js`,
    "// Generated by scripts/build-wasm.mjs: the content version this engine joins with.\n" +
      `export const CONTENT_VERSION = ${JSON.stringify(version)};\n`,
  );
  await writeFile(`${stamp}.d.ts`, "export declare const CONTENT_VERSION: string;\n");
}
