import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import type { Plugin } from "vite";

/**
 * The game page's runtime files under content-hashed names, so a returning visitor
 * keeps them for good: every texture the renderer fetches, and the fonts, sounds and
 * Battle Setup previews the page hands the shell (`sloppyFiles` in `index.html`;
 * natively they are embedded, `crates/desktop/build.rs`). A build emits each file of
 * `HASHED` as `assets/<path>-<hash><ext>` and puts the manifest from its path under
 * `public/` to that URL in the game page, as JSON in `<script id="sloppy-assets">`,
 * which the boot script hands on as `globalThis.sloppyAssets`; the renderer resolves
 * texture paths through it (`assetUrl` in `crates/render/src/gpu/textures.rs`). The
 * dev server's manifest names the plain files, which Vite serves from `public/`, as it
 * still does in builds for the pages without a manifest (the tools and fixtures).
 */
const HASHED = ["textures", "audio", "previews", "fonts"];
/** What the runtime reads; the licences and notes beside the files stay plain. */
const RUNTIME_FILE = /\.(webp|mp3|ttf)$/;
/**
 * Icons the game page links to, which a built page names by hash too. The web app
 * manifest keeps its plain address: a Home Screen app is installed from it and its
 * relative URLs follow the site's base.
 */
const LINKED = ["favicon.svg", "icons/apple-touch-icon.png"];
/** As long as Vite's own: names stay readable and hashes unique among a few dozen. */
const HASH_LENGTH = 8;

const publicDirectory = fileURLToPath(new URL("../public/", import.meta.url));
const gamePage = fileURLToPath(new URL("../index.html", import.meta.url));

/** The runtime files under `public/`, by their path there with `/` separators. */
export function runtimeFiles(root = publicDirectory): string[] {
  const files: string[] = [];
  const walk = (directory: string) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (RUNTIME_FILE.test(entry.name)) files.push(relative(root, path).split(sep).join("/"));
    }
  };
  for (const folder of HASHED) walk(join(root, folder));
  return files.sort();
}

/** `textures/houses/brick.webp` with `bytes` as `assets/textures/houses/brick-<hash>.webp`. */
export function hashedName(path: string, bytes: Uint8Array): string {
  const hash = createHash("sha256").update(bytes).digest("base64url").slice(0, HASH_LENGTH);
  const dot = path.lastIndexOf(".");
  return `assets/${path.slice(0, dot)}-${hash}${path.slice(dot)}`;
}

/** The manifest as an inline JSON script; `<` escaped so no value can close it. */
function manifestScript(manifest: Record<string, string>): string {
  return JSON.stringify(manifest).replaceAll("<", "\\u003c");
}

export function hashedAssets(): Plugin {
  let base = "/";
  let serving = false;
  let manifest: Record<string, string> = {};
  const linked: Record<string, string> = {};
  const emitted: { fileName: string; source: Uint8Array }[] = [];
  const hash = (path: string) => {
    const source = readFileSync(join(publicDirectory, path));
    const fileName = hashedName(path, source);
    emitted.push({ fileName, source });
    return base + fileName;
  };
  return {
    name: "hashed-assets",
    configResolved(config) {
      base = config.base;
      serving = config.command === "serve";
    },
    buildStart() {
      if (serving) return;
      emitted.length = 0;
      manifest = Object.fromEntries(runtimeFiles().map((path) => [path, hash(path)]));
      for (const path of LINKED) linked[path] = hash(path);
    },
    generateBundle() {
      for (const { fileName, source } of emitted) {
        this.emitFile({ type: "asset", fileName, source });
      }
    },
    transformIndexHtml: {
      // After Vite has put the base before the page's links to public files.
      order: "post",
      handler(html, context) {
        if (context.filename !== gamePage) return html;
        // The dev server lists the folders again at each load, so new files show up.
        if (serving) {
          manifest = Object.fromEntries(runtimeFiles().map((path) => [path, base + path]));
        } else {
          for (const [path, url] of Object.entries(linked)) {
            const link = `href="${base}${path}"`;
            if (!html.includes(link)) throw new Error(`index.html has no ${link}`);
            html = html.replaceAll(link, `href="${url}"`);
          }
        }
        return {
          html,
          tags: [
            {
              tag: "script",
              attrs: { type: "application/json", id: "sloppy-assets" },
              children: manifestScript(manifest),
              injectTo: "head",
            },
          ],
        };
      },
    },
  };
}
