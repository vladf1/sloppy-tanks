import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import type { Plugin } from "vite";
import { protocolVersion } from "./content-version.mjs";
import { releaseVersion } from "./release-version.mjs";

/**
 * `health/index.html`: the page's build as JSON, the static counterpart of the
 * multiplayer server's `/health`. Static hosts serve it at `/health` (after a redirect
 * to `/health/` on GitHub Pages) as `text/html`, which JSON readers ignore. The
 * page can join a server only when `version` (the protocol) and `contentVersion` match
 * its; `release` is the version players see.
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
async function healthBody(builtAt: string): Promise<string> {
  // The hash the Wasm build stamped into this engine, not one recomputed from the
  // sources, which may have changed since `pnpm run wasm`.
  const contentVersion = /CONTENT_VERSION = "([0-9a-f]+)"/.exec(
    readFileSync("src/generated/engine/content-version.js", "utf8"),
  )?.[1];
  if (!contentVersion) {
    throw new Error("No content version stamp: run `pnpm run wasm` first");
  }
  const { release, commit, dirty } = pageBuild();
  const health = {
    release,
    version: await protocolVersion(),
    contentVersion,
    commit,
    dirty,
    builtAt,
    style: PRESERVE_FORMATTING,
  };
  return `${JSON.stringify(health, null, 2)}\n`;
}

/** Emits `health/index.html` in builds and answers `health` and `health/` on the dev
 * server, where the page would otherwise fall back to the game's `index.html`. */
export function pageHealth(): Plugin {
  let base = "/";
  return {
    name: "page-health",
    configResolved(config) {
      base = config.base;
    },
    configureServer(server) {
      const started = new Date().toISOString();
      server.middlewares.use((request, response, next) => {
        const path = request.url?.split("?")[0];
        if (path !== `${base}health` && path !== `${base}health/`) return next();
        // Read on each request, so it follows `pnpm run wasm` and new commits.
        healthBody(started).then((body) => {
          response.setHeader("Content-Type", "text/html; charset=utf-8");
          response.setHeader("Cache-Control", "no-store");
          response.end(body);
        }, next);
      });
    },
    async generateBundle() {
      let source: string;
      try {
        source = await healthBody(new Date().toISOString());
      } catch (error) {
        this.error(error instanceof Error ? error.message : String(error));
      }
      this.emitFile({ type: "asset", fileName: "health/index.html", source });
    },
  };
}
