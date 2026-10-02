import { readFileSync } from "node:fs";
import type { Plugin } from "vite";

// Deliberate allowlist: do not publish source directories or Node-only test runners.
export const devPages = [
  {
    // Renders with the labs engine: the dev build needs `pnpm run wasm -- --labs`.
    path: "tools/tank-surface-check.html",
    title: "Tank surfaces",
    detail: "Vehicle textures and materials, drawn by the engine's renderer",
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

/** Links into the game itself, listed beside the pages above; they need no build input. */
const devLinks = [
  {
    href: "/?extralevels",
    title: "Extra levels",
    detail: "Battle Setup with the Stress Grid and Scrap Yard stress levels",
  },
];

export function devSite(): Plugin {
  return {
    name: "dev-site",
    transformIndexHtml: {
      order: "post",
      handler(html) {
        return html.replace(
          "<!-- dev-pages -->",
          [...devLinks, ...devPages.map((page) => ({ ...page, href: `/${page.path}` }))]
            .map(
              ({ href, title, detail }) =>
                `<li><a href="${href}">${title}</a><p>${detail}</p></li>`,
            )
            .join("\n"),
        );
      },
    },
    generateBundle() {
      this.emitFile({
        type: "asset",
        fileName: "_headers",
        source: `${readFileSync("public/_headers", "utf8")}\n/*\n  X-Robots-Tag: noindex, nofollow\n`,
      });
    },
  };
}
