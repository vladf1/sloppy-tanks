import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";
import { build, minify, type Plugin } from "vite";

const entry = fileURLToPath(new URL("../src/main.ts", import.meta.url));
const game = fileURLToPath(new URL("../src/game.ts", import.meta.url));

/** Deliver the authored HTML, CSS and small controller in a single response. */
export function startupHtml(base: string): Plugin {
  let building = false;
  return {
    name: "startup-html",
    configResolved(config) {
      building = config.command === "build";
    },
    buildStart() {
      if (!building) return;
      this.emitFile({ type: "chunk", id: game, preserveSignature: "strict" });
    },
    transformIndexHtml: {
      order: "post",
      async handler(html, context) {
        if (html.includes("<!-- battle-setup -->")) {
          const markup = await readFile(
            new URL("../src/game/battle-setup.html", import.meta.url),
            "utf8",
          );
          html = html.replace(
            "<!-- battle-setup -->",
            `<div id="startup-overlay" data-state="loading">${markup.replaceAll("%BASE_URL%", base)}</div>`,
          );
        }
        if (!html.includes("<!-- startup-script -->")) return html;
        const gameChunk = Object.values(context.bundle ?? {}).find(
          (chunk) => chunk.type === "chunk" && chunk.facadeModuleId === game,
        );
        if (context.bundle && !gameChunk) throw new Error("Missing game entry chunk");
        const gameUrl = `${base}${gameChunk?.fileName ?? "src/game.ts"}`;
        // A separate, small build keeps shared game modules out of the startup
        // dependency graph. The engine remains an external dynamic import.
        const result = await build({
          configFile: false,
          publicDir: false,
          logLevel: "silent",
          base,
          plugins: [
            {
              name: "external-game",
              enforce: "pre",
              resolveId(id) {
                if (id === "./game") return { id: "sloppy:game", external: true };
                return null;
              },
            },
          ],
          build: {
            write: false,
            minify: true,
            modulePreload: false,
            lib: { entry, formats: ["es"] },
          },
        });
        const bundle = Array.isArray(result) ? result[0] : result;
        if (!("output" in bundle)) throw new Error("Expected an inline startup bundle");
        const script = bundle.output.find((chunk) => chunk.type === "chunk");
        const css = bundle.output.find(
          (chunk) => chunk.type === "asset" && chunk.fileName.endsWith(".css"),
        );
        if (script?.type !== "chunk" || css?.type !== "asset") {
          throw new Error("Missing inline startup script or styles");
        }
        const { code } = await minify("startup.js", script.code);
        // The existing stylesheet is only a few KB compressed. Inlining it also
        // keeps the first menu and subsequent game UI on exactly the same styles.
        const style = `<style>${String(css.source).replace(/<\/style/gi, "<\\/style")}</style>`;
        return html
          .replace("</head>", `${style}</head>`)
          .replace(
            "<!-- startup-script -->",
            `<script type="module">${code.replace(/(["'`])sloppy:game\1/g, JSON.stringify(gameUrl)).replace(/<\/script/gi, "<\\/script")}</script>`,
          );
      },
    },
  };
}
