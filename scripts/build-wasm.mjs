// Build the Wasm the pages load: release builds and their wasm-bindgen glue in the
// ignored `src/generated/`. Vite does not compile Rust: rerun this after Rust or WGSL
// edits.
//
// By default it builds the game page: the desktop app's egui shell (`crates/desktop`,
// its `sloppy-tanks` binary) for wasm32, into `src/generated/shell/`, which `index.html`
// imports; the glue finds its binary beside itself, and Vite bundles that as an asset.
// The page loads a second build where the browser gives no WebGPU adapter (`?webgl`
// forces it): the shell's `webgl` feature, into `src/generated/shell-webgl/`, in its own
// target directory (`target/webgl`) since both binaries share a name. Building it doubles
// the compile work of an edit, so by default it is built only while its glue is
// missing (Vite resolves the page's import of it); `--webgl` builds it alone, for trying
// `?webgl` after an edit.
// `--engine` builds the `sloppy-web` engine alone instead, into `src/generated/engine/`:
// the game preview (`tools/game-preview.html`), the `tests/*.browser.html` fixtures and
// the Node multiplayer checks' `WireView` (`scripts/wire-view.mjs`) use it. `--all`
// builds both, as `pnpm run build` does for the type check. `--labs` builds the engine
// with the development labs (`RenderLab`, `EffectsLab`, the `labs` feature of
// `sloppy-web`) into `src/generated/engine-labs/`, so a lab build never replaces the
// engine the fixtures load.
//
// `--fast` builds any of these with the `wasm-fast` Cargo profile instead of release:
// minutes faster after an edit, but larger and slower, so it is for checking how a change
// looks, never for timing, measuring or deploying (Cargo.toml says more).
//
// Every build names its graphics backend (`webgpu` or `webgl`): none is a crate default.
// The shell lists the servers of `SLOPPY_SERVER_ENVIRONMENT` (production unless set;
// `build:dev` sets `dev`) from `SLOPPY_DIRECTORY_URL`, both read when it compiles.
//
// Cargo runs from the repository root so `.cargo/config.toml` (SIMD) applies.
// Setup: rustup's wasm32-unknown-unknown target and `wasm-bindgen-cli` 0.2.129,
// matching the `wasm-bindgen` crate pin.
//
// Like the server build, the Wasm is stamped with the content version clients and the
// server must share (`SLOPPY_CONTENT_VERSION`): multiplayer joins send it. The shell
// also carries the release and commit Battle Setup shows, as the desktop app does, and
// its content version goes to `src/generated/shell/content-version` for the page's
// `/health` (`scripts/page-health.ts`).
import { spawn, spawnSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { contentVersion } from "./content-version.mjs";
import { releaseVersion } from "./release-version.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const version = await contentVersion();

const SHELL = {
  package: "sloppy-desktop",
  binary: "sloppy-tanks",
  features: ["--features", "webgpu"],
  dir: "src/generated/shell",
  outName: "sloppy-tanks",
};
const SHELL_WEBGL = {
  package: "sloppy-desktop",
  binary: "sloppy-tanks",
  features: ["--features", "webgl"],
  targetDir: "target/webgl",
  dir: "src/generated/shell-webgl",
  outName: "sloppy-tanks-webgl",
};
const ENGINE = {
  package: "sloppy-web",
  binary: "sloppy_web",
  features: ["--features", "webgpu,js-api"],
  dir: "src/generated/engine",
  outName: "engine",
};
const LABS = {
  package: "sloppy-web",
  binary: "sloppy_web",
  features: ["--features", "webgpu,labs"],
  dir: "src/generated/engine-labs",
  outName: "engine",
};

/** A command's trimmed output, or "" when it fails. */
function capture(command, args) {
  const result = spawnSync(command, args, { cwd: root, encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : "";
}

const fast = process.argv.includes("--fast");
const profile = fast ? "wasm-fast" : "release";

const commit = capture("git", ["rev-parse", "--short", "HEAD"]) || "unknown";
const dirty = capture("git", ["status", "--porcelain"]) !== "";
const stamps = {
  SLOPPY_CONTENT_VERSION: version,
  SLOPPY_RELEASE: releaseVersion(process.env.SLOPPY_BUILD_NUMBER),
  SLOPPY_COMMIT: dirty ? `${commit}-dirty` : commit,
};

/** Run `command` from the repository root; rejects unless it succeeds. */
function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: root,
      stdio: "inherit",
      env: { ...process.env, ...stamps },
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

/** Build `package` with `features` in `targetDir`, then write the glue of its `binary`
 * as `outName` in `dir`. */
async function build({ package: name, binary, features, targetDir = "target", dir, outName }) {
  await run("cargo", [
    "build",
    "--locked",
    "--profile",
    profile,
    "--target",
    "wasm32-unknown-unknown",
    "--target-dir",
    targetDir,
    "-p",
    name,
    ...features,
  ]);
  await run("wasm-bindgen", [
    `${targetDir}/wasm32-unknown-unknown/${profile}/${binary}.wasm`,
    "--target",
    "web",
    "--out-dir",
    dir,
    "--out-name",
    outName,
  ]);
}

const webglGlue = existsSync(
  fileURLToPath(new URL(`../${SHELL_WEBGL.dir}/${SHELL_WEBGL.outName}.js`, import.meta.url)),
);
const builds = process.argv.includes("--labs")
  ? [LABS]
  : process.argv.includes("--engine")
    ? [ENGINE]
    : process.argv.includes("--webgl")
      ? [SHELL_WEBGL]
      : process.argv.includes("--all")
        ? [SHELL, SHELL_WEBGL, ENGINE]
        : webglGlue
          ? [SHELL]
          : [SHELL, SHELL_WEBGL];
// Shader tables ------------------------------------------------------------------------
// The page's shell draws shader variants the build generated rather than assembling them
// (crates/render/src/shader/tables.rs): before it builds, the native `shader-tables`
// generator writes every variant either backend can ask for into `src/generated/shaders/`,
// which index.html fetches beside the binary. It runs again only once Cargo rebuilt it,
// which a change to the renderer's sources does (about 3 s).
const SHADER_TABLES = "src/generated/shaders";

async function shaderTables() {
  await run("cargo", ["build", "--locked", "-p", "sloppy-render", "--features", "shader-tables"]);
  const target = process.env.CARGO_TARGET_DIR ?? "target";
  const generator = `${target}/debug/shader-tables${process.platform === "win32" ? ".exe" : ""}`;
  const at = (path) => resolve(root, path);
  const stamp = at(`${SHADER_TABLES}/generator`);
  const built = String(statSync(at(generator)).mtimeMs);
  const current = ["webgpu.json", "webgl.json", "generator"].every((file) =>
    existsSync(at(`${SHADER_TABLES}/${file}`)),
  );
  if (current && readFileSync(stamp, "utf8") === built) return;
  await run(at(generator), [SHADER_TABLES]);
  await writeFile(stamp, built);
}
// ---------------------------------------------------------------------------------------

try {
  if (builds.includes(SHELL) || builds.includes(SHELL_WEBGL)) await shaderTables();
  for (const target of builds) {
    await build(target);
  }
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
if (builds.includes(SHELL)) {
  await writeFile(
    fileURLToPath(new URL(`../${SHELL.dir}/content-version`, import.meta.url)),
    version,
  );
}
