import { spawnSync } from "node:child_process";
import { readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { contentVersion, siteBuild } from "./content-version.mjs";
import { precompress } from "./precompress.mjs";

/** The multiplayer server's own copy of the game page, `dist-server/`, which its image
 * ships and serves (crates/server/src/site.rs). One copy serves every server: it is built
 * for the site root and joins the server that served it (`VITE_MULTIPLAYER_URL=/`).
 * `node scripts/server-site.mjs` builds it from the engines `pnpm run wasm:all` built. */
const repo = fileURLToPath(new URL("..", import.meta.url));
export const SERVER_SITE_DIRECTORY = join(repo, "dist-server");
/** Battle Setup's build line in this copy. The server writes in its own release and
 * commit, so the copy's bytes, which name its image, change only with the page itself
 * and a commit that leaves the page alone does not restart the server. */
export const BUILD_LABEL_PLACEHOLDER = "<!--sloppy-build-label-->";
/** Text and the engines shrink three- to fourfold; WebP, PNG and MP3 are compressed. */
const COMPRESSIBLE = /\.(html|js|mjs|css|svg|json|webmanifest|txt|md|wasm)$/;
/** Below this a stored copy saves a few hundred bytes at most. */
const MIN_PRECOMPRESSED_BYTES = 1024;

/** Builds `dist-server/` with Brotli and gzip copies and returns its site build.
 * `buildWasm` first builds both engines from this checkout, as a local image build must:
 * the content version below covers only `crates/core`, so engines left from before a
 * renderer or shader change would pass it, and `pnpm run wasm` alone rebuilds the WebGL
 * fallback only when its API changes. CI's site job built both moments earlier. */
export async function buildServerSite({ buildWasm = false } = {}) {
  if (buildWasm) {
    const wasm = spawnSync("node", ["scripts/build-wasm.mjs", "--all"], {
      cwd: repo,
      stdio: "inherit",
    });
    if (wasm.error) throw wasm.error;
    if (wasm.status !== 0) throw new Error(`The Wasm build exited with ${wasm.status}`);
  }
  // The image ships this page beside a server built from the same checkout, and the two
  // refuse each other unless the engine was built from these sources.
  const stamp = /CONTENT_VERSION = "([0-9a-f]+)"/.exec(
    await readFile(join(repo, "src/generated/engine/content-version.js"), "utf8"),
  )?.[1];
  if (stamp !== (await contentVersion())) {
    throw new Error("The Wasm engines are missing or stale: run `pnpm run wasm:all` first");
  }
  const vite = spawnSync(
    "pnpm",
    ["exec", "vite", "build", "--outDir", SERVER_SITE_DIRECTORY, "--logLevel", "warn"],
    {
      cwd: repo,
      stdio: "inherit",
      env: { ...process.env, DEPLOY_BASE: "/", VITE_MULTIPLAYER_URL: "/", SLOPPY_SERVER_SITE: "1" },
    },
  );
  if (vite.error) throw vite.error;
  if (vite.status !== 0) throw new Error(`vite build exited with ${vite.status}`);
  // The server answers /health itself.
  await rm(join(SERVER_SITE_DIRECTORY, "health"), { recursive: true, force: true });
  // The page whose build line the server fills in is compressed by the server instead.
  await precompress(
    SERVER_SITE_DIRECTORY,
    (path, bytes) =>
      COMPRESSIBLE.test(path) &&
      bytes.length >= MIN_PRECOMPRESSED_BYTES &&
      !bytes.includes(BUILD_LABEL_PLACEHOLDER),
  );
  return siteBuild(SERVER_SITE_DIRECTORY);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  console.log(`Built dist-server/ (site build ${await buildServerSite()})`);
}
