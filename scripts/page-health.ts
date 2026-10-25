import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { protocolVersion } from "./content-version.mjs";
import { releaseVersion } from "./release-version.mjs";

/**
 * `health/index.html`: the page's build as JSON, the static counterpart of the
 * multiplayer server's `/health`. Static hosts serve it at `/health` (after a redirect
 * to `/health/` on GitHub Pages) as `text/html`, which JSON readers ignore. The
 * page can join a server only when `protocol` and `contentVersion` match its; `version`
 * is the release version players see.
 */
// HTML collapses the indentation, and GitHub Pages cannot send a JSON content type, so
// a string value carries a style element that keeps it. Browsers show it as `""`.
const PRESERVE_FORMATTING = "<style>body{white-space:pre;font-family:monospace}</style>";

export interface PageBuild {
  /** `1.1.0.628` on main, with the Pages workflow's run number (`SLOPPY_BUILD_NUMBER`). */
  release: string;
  commit: string;
  dirty: boolean;
}

/** What this checkout builds: its release version and commit. */
export function pageBuild(): PageBuild {
  const git = (...args: string[]) => execFileSync("git", args, { encoding: "utf8" }).trim();
  return {
    release: releaseVersion(process.env.SLOPPY_BUILD_NUMBER),
    commit: git("rev-parse", "--short", "HEAD"),
    dirty: Boolean(git("status", "--porcelain")),
  };
}

/** Battle Setup's footer line: `v1.1.0.628 · 696497f`, `+` marking local changes. */
export function buildLabel({ release, commit, dirty }: PageBuild): string {
  return `v${release} · ${commit}${dirty ? "+" : ""}`;
}

/** The page's `/health` body; `builtAt` is when the build ran or the dev server started. */
export async function healthBody(builtAt: string): Promise<string> {
  // The hash the Wasm build stamped into this engine, not one recomputed from the
  // sources, which may have changed since `bun run wasm`.
  const contentVersion = /CONTENT_VERSION = "([0-9a-f]+)"/.exec(
    readFileSync("src/generated/engine/content-version.js", "utf8"),
  )?.[1];
  if (!contentVersion) {
    throw new Error("No content version stamp: run `bun run wasm` first");
  }
  const { release, commit, dirty } = pageBuild();
  const health = {
    version: release,
    protocol: await protocolVersion(),
    contentVersion,
    commit,
    dirty,
    builtAt,
    style: PRESERVE_FORMATTING,
  };
  return `${JSON.stringify(health, null, 2)}\n`;
}
