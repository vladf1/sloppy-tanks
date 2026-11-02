import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { readdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);
const repo = fileURLToPath(root);

/** Every file under `directory` (relative to the repository root), sorted. */
async function filesUnder(directory) {
  const entries = await readdir(new URL(directory, root), { recursive: true, withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile())
    .map((entry) => `${entry.parentPath}/${entry.name}`.slice(repo.length).split("\\").join("/"))
    .sort();
}

/** The resolved crates (name, version, enabled features) a package compiles with. The
 * lock file also pins crates only other packages use, so hashing it whole would tie
 * client compatibility to server-only or renderer-only dependency updates. */
function dependencyTree(pkg, target) {
  const args = ["tree", "--locked", "-p", pkg, "-e", "normal", "--prefix", "none"];
  if (target) args.push("--target", target);
  args.push("-f", "{p} {f}");
  const result = spawnSync("cargo", args, { cwd: repo, encoding: "utf8" });
  if (result.status !== 0) {
    throw new Error(`cargo tree failed for ${pkg}: ${result.stderr || result.error?.message}`);
  }
  // Local packages print their absolute path, which differs between checkouts.
  const lines = result.stdout
    .split("\n")
    .map((line) => line.replace(/\s*\([^)]*\)/g, "").trim())
    .filter(Boolean);
  return [...new Set(lines)].sort();
}

async function hashFiles(paths, extra = [], base = root) {
  const hash = createHash("sha256");
  for (const path of paths) {
    hash.update(path + "\0").update(await readFile(new URL(path, base)));
  }
  for (const value of extra) hash.update(value + "\n");
  return hash.digest("hex").slice(0, 24);
}

/** Sources the multiplayer server runs that the browser build shares: the whole core
 * crate (rules, simulation, models it measures, protocol and replication). Only these
 * can make a client and server disagree about the game, so renderer, browser-binding
 * and server-only code stay out and never force a server redeploy. Integration tests
 * and examples never reach either build. */
export async function contentSources() {
  const files = await filesUnder("crates/core");
  return files.filter(
    (path) => !path.startsWith("crates/core/tests/") && !path.startsWith("crates/core/examples/"),
  );
}

/** The files a rustc dep-info file lists for its target (relative to `base`, the directory
 * Cargo ran in, when inside it) and the environment variables the compiled code reads
 * (its `# env-dep:` lines, as `NAME=value`). */
export function parseDepInfo(text, base = repo) {
  const lines = text.split("\n");
  const rule = lines.find((line) => line.trim() && !line.startsWith("#")) ?? "";
  const files = rule
    .slice(rule.indexOf(": ") + 2)
    .split(/(?<!\\) /)
    .map((path) => path.replaceAll("\\ ", " ").trim())
    .filter(Boolean)
    .map((path) => (path.startsWith(base) ? path.slice(base.length) : path))
    .sort();
  const env = lines
    .filter((line) => line.startsWith("# env-dep:"))
    .map((line) => line.slice("# env-dep:".length))
    .sort();
  return { files, env };
}

/** The sloppy-core sources the page's cached models depend on: the files that the crate's
 * `--cfg sloppy_geometry_only` build compiles (its dep-info), which leaves out everything
 * the cached builders (`build_scenery`, `tank_model_variant`) and their codec cannot
 * reach, and fails if they come to reach it (`beyond_geometry!` in crates/core/src/lib.rs).
 * Built in its own target directory, so it never invalidates the usual builds. */
export function geometrySources() {
  const result = spawnSync(
    "cargo",
    [
      "rustc",
      "--locked",
      "-p",
      "sloppy-core",
      "--lib",
      "--profile",
      "check",
      "--target-dir",
      "target/geometry-only",
      "--message-format",
      "json-render-diagnostics",
      "--",
      "--cfg",
      "sloppy_geometry_only",
    ],
    { cwd: repo, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  );
  if (result.status !== 0) {
    throw new Error(
      `The geometry-only build of sloppy-core failed: a cached model builder reaches code ` +
        `outside it (see beyond_geometry! in crates/core/src/lib.rs).\n${result.stderr}`,
    );
  }
  const artifact = result.stdout
    .split("\n")
    .filter((line) => line.startsWith("{"))
    .map((line) => JSON.parse(line))
    .find(
      (message) => message.reason === "compiler-artifact" && message.target?.name === "sloppy_core",
    );
  const metadata = artifact?.filenames.find((file) => file.endsWith(".rmeta"));
  if (!metadata) throw new Error("The geometry-only build named no sloppy-core metadata");
  // `deps/libsloppy_core-<hash>.rmeta` beside `deps/sloppy_core-<hash>.d`.
  const depInfo = metadata.replace(/lib(sloppy_core-[0-9a-f]+)\.rmeta$/, "$1.d");
  const sources = parseDepInfo(readFileSync(depInfo, "utf8"));
  const reached = sources.files.filter((file) =>
    OUTSIDE_GEOMETRY.some((path) => file.startsWith(path)),
  );
  if (reached.length) {
    throw new Error(
      `The cached models must not depend on the simulation or the balance, but the ` +
        `geometry-only build compiles ${reached.join(", ")}: cut the edge rather than ` +
        `let gameplay changes empty every player's cache.`,
    );
  }
  return sources;
}

/** Sources the cached models must never reach, so the geometry-only build cannot grow
 * back into the simulation to make an edge compile: gameplay changes would empty every
 * player's cache again. */
const OUTSIDE_GEOMETRY = [
  "crates/core/src/net/",
  "crates/core/src/sim/data.rs",
  "crates/core/src/sim/physics.rs",
  "crates/core/src/sim/render_state.rs",
  "crates/core/src/sim/simulation.rs",
  "crates/core/src/sim/types.rs",
];

/** The root manifest's `[profile.*]` sections: how every build compiles. */
async function cargoProfiles() {
  const manifest = await readFile(new URL("Cargo.toml", root), "utf8");
  return manifest
    .split(/^(?=\[)/m)
    .filter((section) => section.startsWith("[profile"))
    .join("");
}

/** A key over `files` (relative to `base`) and `extra` values. */
export function geometryKey(files, extra, base = root) {
  return hashFiles(files, extra, base);
}

/** The page's geometry cache key (`SLOPPY_GEOMETRY_VERSION`): what the cached vehicle and
 * scenery models can depend on, and nothing else, so a release that changes no model
 * keeps every player's cache. That is the sources the geometry-only build compiles (with
 * the codec's FORMAT_VERSION in `scene_codec.rs`), the environment they read,
 * sloppy-core's manifest and resolved crates, the compiler, the profiles and target
 * flags, and the profile the page is built with. The cached vehicles also give the
 * simulation its hitboxes (`tank_hull`), so the key must be complete:
 * `tests/content-version.test.ts` checks it follows the build's files. */
export async function geometryVersion(profile) {
  const { files, env } = geometrySources();
  return geometryKey(
    [...files, "crates/core/Cargo.toml", "rust-toolchain.toml", ".cargo/config.toml"],
    [...env, ...dependencyTree("sloppy-core", "all"), await cargoProfiles(), `profile ${profile}`],
  );
}

/** What the page's shader tables (`crates/render/src/shader/tables.rs`) are generated
 * from: the renderer's sources, build script and manifest, the core's `scene` module
 * (the material defaults that pick shadow keys), the resolved crates (naga translates
 * WebGL's stages) and the compiler. The rest of the core never reaches them, so
 * `build-wasm.mjs` regenerates the tables only when their key changes, and a core edit
 * no longer rebuilds the native generator first. */
export async function shaderTablesSources() {
  return [
    ...(await filesUnder("crates/render/src")),
    "crates/render/build.rs",
    "crates/render/Cargo.toml",
    "crates/core/src/scene.rs",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
  ];
}

/** The shader tables' key: a hash of `shaderTablesSources`. */
export async function shaderTablesKey() {
  return hashFiles(await shaderTablesSources());
}

/** The network protocol version, a constant of the shared Rust core both builds compile. */
export async function protocolVersion() {
  const source = await readFile(new URL("crates/core/src/net/protocol.rs", root), "utf8");
  const version = Number(/pub const PROTOCOL_VERSION: u32 = (\d+);/.exec(source)?.[1]);
  if (!Number.isInteger(version)) throw new Error("PROTOCOL_VERSION not found in protocol.rs");
  return version;
}

/** Clients and the server must match on this: the shared sources, the crates they
 * compile with (Rapier, glam, serde) and the compiler that builds both. The tree is
 * resolved for every target, so the hash is the same on whichever machine builds it
 * (x86 hosts otherwise add platform-only crates such as `safe_arch`). */
export async function contentVersion() {
  return hashFiles(
    [...(await contentSources()), "rust-toolchain.toml"],
    dependencyTree("sloppy-core", "all"),
  );
}

/** The server crate's own build inputs. Its guide and other Markdown never reach the
 * binary, and a new hash promotes a new image whose rollout restarts the server and
 * ends live rooms, so a docs edit must leave the build unchanged. */
export async function serverSources() {
  const files = await filesUnder("crates/server");
  return files.filter((path) => !path.startsWith("crates/server/tests/") && !path.endsWith(".md"));
}

/** Everything that makes up the deployed server, including server-only code, its
 * dependencies and build settings, which never affect client compatibility. A change
 * here needs a server redeploy to take effect even when `contentVersion` matches. */
export async function serverBuild() {
  return hashFiles(
    [
      ...(await contentSources()),
      ...(await serverSources()),
      "Cargo.toml",
      ".cargo/config.toml",
      "rust-toolchain.toml",
      "scripts/build-server.mjs",
      // CI names server images by this hash and skips builds the registry already holds,
      // so the image recipe, its build context and the arguments it gets must change it too.
      "Dockerfile",
      ".dockerignore",
      "scripts/build-server-image.mjs",
    ],
    dependencyTree("sloppy-server", "x86_64-unknown-linux-musl"),
  );
}

// `bun scripts/content-version.mjs` lists the sources clients and the server share;
// `--geometry` the sources of the page's cached models and their key (a CI check: it fails
// when a cached model builder reaches beyond them).
if (process.argv[1] === fileURLToPath(import.meta.url) && process.argv.includes("--geometry")) {
  const { files, env } = geometrySources();
  console.log([...files, ...env].join("\n"));
  console.log(`geometry ${await geometryVersion("release")} (${files.length} files)`);
} else if (process.argv[1] === fileURLToPath(import.meta.url)) {
  console.log((await contentSources()).join("\n"));
  console.log(`content ${await contentVersion()}`);
  console.log(`server build ${await serverBuild()}`);
}
