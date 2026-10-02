import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import type { Plugin } from "vite";
import { protocolVersion } from "./content-version.mjs";

/**
 * `health/index.html`: the page's build as JSON, the static counterpart of the
 * multiplayer server's `/health`. Static hosts serve it at `/health` (after a redirect
 * to `/health/` on GitHub Pages) as `text/html`, which JSON readers ignore. The
 * page can join a server only when `version` and `contentVersion` match its.
 */
// HTML collapses the indentation, and GitHub Pages cannot send a JSON content type, so
// a string value carries a style element that keeps it. Browsers show it as `""`.
const PRESERVE_FORMATTING = "<style>body{white-space:pre;font-family:monospace}</style>";

export function pageHealth(): Plugin {
  return {
    name: "page-health",
    apply: "build",
    async generateBundle() {
      // The hash the Wasm build stamped into this engine, not one recomputed from the
      // sources, which may have changed since `pnpm run wasm`.
      const contentVersion = /CONTENT_VERSION = "([0-9a-f]+)"/.exec(
        readFileSync("src/generated/engine/content-version.js", "utf8"),
      )?.[1];
      if (!contentVersion) {
        this.error("No content version stamp: run `pnpm run wasm` first");
      }
      const git = (...args: string[]) => execFileSync("git", args, { encoding: "utf8" }).trim();
      const health = {
        version: await protocolVersion(),
        contentVersion,
        commit: git("rev-parse", "--short", "HEAD"),
        dirty: Boolean(git("status", "--porcelain")),
        builtAt: new Date().toISOString(),
        style: PRESERVE_FORMATTING,
      };
      this.emitFile({
        type: "asset",
        fileName: "health/index.html",
        source: `${JSON.stringify(health, null, 2)}\n`,
      });
    },
  };
}
