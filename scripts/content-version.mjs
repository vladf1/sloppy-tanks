import { build } from "esbuild";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);
/** Files the server build reads besides its import graph: the inlined dashboard page
 * and the bundling settings themselves. */
const SERVER_BUILD_FILES = ["server/build.mjs", "server/dashboard.html"];

let inputs;
/** Every file in the server bundle's import graph, relative to the repository root,
 * including the bundled packages under node_modules (their paths carry versions). */
function serverInputs() {
  inputs ??= build({
    entryPoints: [fileURLToPath(new URL("server/main.ts", root))],
    absWorkingDir: fileURLToPath(root),
    bundle: true,
    platform: "node",
    format: "esm",
    external: ["bufferutil", "utf-8-validate"],
    write: false,
    metafile: true,
    logLevel: "silent",
  }).then((result) => Object.keys(result.metafile.inputs).sort());
  return inputs;
}

async function hashFiles(paths, extra = []) {
  const hash = createHash("sha256");
  for (const path of paths) {
    hash.update(path + "\0").update(await readFile(new URL(path, root)));
  }
  for (const value of extra) hash.update(value);
  return hash.digest("hex").slice(0, 24);
}

async function pinnedVersions(names) {
  const pkg = JSON.parse(await readFile(new URL("package.json", root), "utf8"));
  return names.map((name) => name + "=" + (pkg.dependencies[name] ?? pkg.devDependencies[name]));
}

/** Repository sources the multiplayer server runs that the client build shares.
 * Only these can make a client and server disagree about the game, so client-only
 * presentation, input and UI files stay out of the hash and never force a server
 * redeploy. Server-only files under `server/` are left out for the same reason. */
export async function contentSources() {
  return (await serverInputs()).filter((path) => path.startsWith("src/"));
}

/** Clients and the server must match on this: the shared sources and pinned engines. */
export async function contentVersion() {
  return hashFiles(await contentSources(), await pinnedVersions(["@dimforge/rapier3d", "three"]));
}

/** Everything that makes up the deployed server, including server-only code and
 * dependencies that never affect client compatibility. A change here needs a server
 * redeploy to take effect even when `contentVersion` still matches. */
export async function serverBuild() {
  return hashFiles(
    [...(await serverInputs()), ...SERVER_BUILD_FILES],
    await pinnedVersions(["esbuild"]),
  );
}

// `node scripts/content-version.mjs` lists the sources clients and the server share.
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  console.log((await contentSources()).join("\n"));
  console.log(`content ${await contentVersion()}`);
  console.log(`server build ${await serverBuild()}`);
}
