// Binaryen's `wasm-split` and `wasm-opt`, which split the game page's binary into its
// menu and engine halves (`scripts/split-wasm.mjs`). No package manager ships
// wasm-split, so this downloads Binaryen's own release for this machine once, checks it
// against the release's SHA-256 below and keeps the two tools (and, on macOS, the
// library they link) in the user's cache, where every checkout and CI run finds them:
// no setup locally or in CI. `SLOPPY_BINARYEN` names a Binaryen installation to use
// instead (the directory holding `bin/wasm-split`), for a machine without the network.
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, rename, rm, writeFile } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import path from "node:path";

export const BINARYEN_VERSION = "version_133";

/** The release archive for each platform and its SHA-256, from the release's `.sha256`. */
const ARCHIVES = {
  "darwin-arm64": [
    "arm64-macos",
    "ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8",
  ],
  "darwin-x64": [
    "x86_64-macos",
    "13a9b90be775c6389ce3d1f879cb8627bea56708ba8c122983941d53a8199b95",
  ],
  "linux-x64": ["x86_64-linux", "2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489"],
  "linux-arm64": [
    "aarch64-linux",
    "89c07ea56faf38d0fbecf36ca8ec0721756716185f265b568e133d427f299bf8",
  ],
  "win32-x64": [
    "x86_64-windows",
    "17a2cbeac6b5693c5fbafab3838d3c65fd9c1eb38b05f5baec6c657e8c84995b",
  ],
  "win32-arm64": [
    "arm64-windows",
    "492a8e1847a0be1554bb9a7f384227981d60bc013aedc02d8ba1372c3943178c",
  ],
};

const executable = (name) => (process.platform === "win32" ? `${name}.exe` : name);

/** Where downloaded tools stay between builds. */
function cacheDirectory() {
  if (process.env.XDG_CACHE_HOME) return path.join(process.env.XDG_CACHE_HOME, "sloppy-tanks");
  if (process.platform === "darwin")
    return path.join(homedir(), "Library", "Caches", "sloppy-tanks");
  if (process.platform === "win32" && process.env.LOCALAPPDATA) {
    return path.join(process.env.LOCALAPPDATA, "sloppy-tanks");
  }
  return path.join(homedir(), ".cache", "sloppy-tanks");
}

/** The tools in a Binaryen installation `root`, when it has them. */
function toolsIn(root) {
  const tools = {
    wasmSplit: path.join(root, "bin", executable("wasm-split")),
    wasmOpt: path.join(root, "bin", executable("wasm-opt")),
  };
  return existsSync(tools.wasmSplit) && existsSync(tools.wasmOpt) ? tools : null;
}

/** Download and unpack this platform's release into `root`. */
async function install(root) {
  const archive = ARCHIVES[`${process.platform}-${process.arch}`];
  if (!archive) {
    throw new Error(
      `No Binaryen release for ${process.platform}-${process.arch}: set SLOPPY_BINARYEN to a Binaryen ${BINARYEN_VERSION} installation`,
    );
  }
  const [platform, sha256] = archive;
  const name = `binaryen-${BINARYEN_VERSION}-${platform}.tar.gz`;
  const url = `https://github.com/WebAssembly/binaryen/releases/download/${BINARYEN_VERSION}/${name}`;
  console.log(`Downloading Binaryen ${BINARYEN_VERSION} (wasm-split) from ${url}`);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`Binaryen download failed: HTTP ${response.status} for ${url}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  const digest = createHash("sha256").update(bytes).digest("hex");
  if (digest !== sha256) {
    throw new Error(`Binaryen download ${name} has SHA-256 ${digest}, expected ${sha256}`);
  }
  await mkdir(path.dirname(root), { recursive: true });
  const staging = await mkdtemp(path.join(tmpdir(), "sloppy-binaryen-"));
  try {
    const file = path.join(staging, name);
    await writeFile(file, bytes);
    const prefix = `binaryen-${BINARYEN_VERSION}`;
    // Only the two tools, and the shared library the macOS tools link against.
    const members = [
      `${prefix}/bin/${executable("wasm-split")}`,
      `${prefix}/bin/${executable("wasm-opt")}`,
    ];
    if (process.platform === "darwin") members.push(`${prefix}/lib`);
    const unpacked = spawnSync("tar", ["-xzf", file, "-C", staging, ...members], {
      encoding: "utf8",
    });
    if (unpacked.status !== 0) throw new Error(`Unpacking ${name} failed: ${unpacked.stderr}`);
    await rm(root, { recursive: true, force: true });
    try {
      await rename(path.join(staging, prefix), root);
    } catch (error) {
      // Another build (a second checkout) installed it meanwhile.
      if (!toolsIn(root)) throw error;
    }
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}

/** `wasmSplit` and `wasmOpt`: the paths of Binaryen's tools, downloaded on first use. */
export async function binaryen() {
  if (process.env.SLOPPY_BINARYEN) {
    const tools = toolsIn(process.env.SLOPPY_BINARYEN);
    if (!tools)
      throw new Error(`SLOPPY_BINARYEN=${process.env.SLOPPY_BINARYEN} has no bin/wasm-split`);
    return tools;
  }
  const root = path.join(cacheDirectory(), `binaryen-${BINARYEN_VERSION}`);
  let tools = toolsIn(root);
  if (!tools) {
    await install(root);
    tools = toolsIn(root);
    if (!tools) throw new Error(`Binaryen ${BINARYEN_VERSION} unpacked without wasm-split`);
  }
  return tools;
}
