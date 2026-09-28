import { build } from "esbuild";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);

/** Repository sources the multiplayer server runs that the client build shares.
 * Only these can make a client and server disagree about the game, so client-only
 * presentation, input and UI files stay out of the hash and never force a server
 * redeploy. Server-only files under `server/` are left out for the same reason. */
export async function contentSources() {
  const result = await build({
    entryPoints: [fileURLToPath(new URL("server/main.ts", root))],
    absWorkingDir: fileURLToPath(root),
    bundle: true,
    platform: "node",
    format: "esm",
    external: ["bufferutil", "utf-8-validate"],
    write: false,
    metafile: true,
    logLevel: "silent",
  });
  return Object.keys(result.metafile.inputs)
    .filter((path) => path.startsWith("src/"))
    .sort();
}

/** Both builds hash the shared server sources and pinned engine versions. */
export async function contentVersion() {
  const hash = createHash("sha256");
  for (const path of await contentSources()) {
    hash.update(path + "\0").update(await readFile(new URL(path, root)));
  }
  const pkg = JSON.parse(await readFile(new URL("package.json", root), "utf8"));
  for (const name of ["@dimforge/rapier3d", "three"])
    hash.update(name + "=" + pkg.dependencies[name]);
  return hash.digest("hex").slice(0, 24);
}

// `node scripts/content-version.mjs` lists what a server redeploy depends on.
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  console.log((await contentSources()).join("\n"));
  console.log(`content ${await contentVersion()}`);
}
