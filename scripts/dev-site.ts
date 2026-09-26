import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import type { Plugin } from "vite";

// Deliberate allowlist: do not publish source directories or Node-only test runners.
export const devPages = [
  { path: "stresstest.html", title: "Stress test", detail: "30 tanks and continuous combat" },
  {
    path: "tools/tank-surface-check.html",
    title: "Tank surfaces",
    detail: "Vehicle textures and materials",
  },
  ...["humvee", "suspension", "maps", "destruction", "reinforcements"].map((name) => ({
    path: `tests/${name}.browser.html`,
    title: `${name[0].toUpperCase()}${name.slice(1)} check`,
    detail:
      name === "reinforcements"
        ? "Automatic rendering regression with a pass/fail result"
        : name === "maps" || name === "suspension"
          ? "Interactive inspection with pass/fail checks (run by fixtures-check.mjs)"
          : "Interactive browser inspection and regression fixture",
  })),
];

export function devSite(): Plugin {
  const commit = execFileSync("git", ["rev-parse", "--short", "HEAD"], { encoding: "utf8" }).trim();
  const dirty = Boolean(
    execFileSync("git", ["status", "--porcelain"], { encoding: "utf8" }).trim(),
  );
  const builtAt = new Date().toISOString();
  return {
    name: "dev-site",
    transformIndexHtml: {
      order: "post",
      handler(html) {
        return html.replace(
          "<!-- dev-pages -->",
          devPages
            .map(
              ({ path, title, detail }) =>
                `<li><a href="/${path}">${title}</a><p>${detail}</p></li>`,
            )
            .join("\n"),
        );
      },
    },
    generateBundle() {
      this.emitFile({
        type: "asset",
        fileName: "build-info.json",
        source: JSON.stringify({ builtAt, commit, dirty }, null, 2),
      });
      this.emitFile({
        type: "asset",
        fileName: "_headers",
        source: `${readFileSync("public/_headers", "utf8")}\n/*\n  X-Robots-Tag: noindex, nofollow\n`,
      });
    },
  };
}
