import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
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

async function hashFiles(paths, extra = []) {
  const hash = createHash("sha256");
  for (const path of paths) {
    hash.update(path + "\0").update(await readFile(new URL(path, root)));
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

/** Everything that makes up the deployed server, including server-only code, its
 * dependencies and build settings, which never affect client compatibility. A change
 * here needs a server redeploy to take effect even when `contentVersion` matches. */
export async function serverBuild() {
  return hashFiles(
    [
      ...(await contentSources()),
      ...(await filesUnder("crates/server")).filter(
        (path) => !path.startsWith("crates/server/tests/"),
      ),
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

// `node scripts/content-version.mjs` lists the sources clients and the server share.
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  console.log((await contentSources()).join("\n"));
  console.log(`content ${await contentVersion()}`);
  console.log(`server build ${await serverBuild()}`);
}
