import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";
import { build, minify, type Plugin } from "vite";

const entry = fileURLToPath(new URL("../src/main.ts", import.meta.url));
const game = fileURLToPath(new URL("../src/game.ts", import.meta.url));
const multiplayer = fileURLToPath(new URL("../src/net/client.ts", import.meta.url));

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
      this.emitFile({ type: "chunk", id: multiplayer, preserveSignature: "strict" });
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
        const multiplayerChunk = Object.values(context.bundle ?? {}).find(
          (chunk) => chunk.type === "chunk" && chunk.facadeModuleId === multiplayer,
        );
        if (context.bundle && !multiplayerChunk) throw new Error("Missing multiplayer entry chunk");
        const multiplayerUrl = `${base}${multiplayerChunk?.fileName ?? "src/net/client.ts"}`;
        // These entries are external to the inline build, so Vite cannot attach
        // its usual dynamic-import CSS loader. Load each entry's static CSS
        // graph before starting it, and only when that mode is selected.
        const entryImport = (url: string, fileName?: string) => {
          const visited = new Set<string>();
          const styles = new Set<string>();
          const collectStyles = (file: string) => {
            const chunk = context.bundle?.[file];
            if (chunk?.type !== "chunk" || visited.has(file)) return;
            visited.add(file);
            chunk.imports.forEach(collectStyles);
            chunk.viteMetadata?.importedCss.forEach((css) => styles.add(base + css));
          };
          if (fileName) collectStyles(fileName);
          const load = `import(${JSON.stringify(url)})`;
          if (!styles.size) return load;
          return `Promise.all([${load},...${JSON.stringify([...styles])}.map(href=>new Promise((resolve,reject)=>{const link=document.createElement("link");link.rel="stylesheet";link.href=href;link.onload=resolve;link.onerror=()=>reject(new Error("Could not load "+href));document.head.append(link)}))]).then(([entry])=>entry)`;
        };
        // A separate, small build keeps shared game modules out of the startup
        // dependency graph. The engine remains an external dynamic import.
        const result = await build({
          configFile: false,
          publicDir: false,
          logLevel: "silent",
          base,
          define: {
            "import.meta.env.VITE_MULTIPLAYER_URL": JSON.stringify(
              process.env.VITE_MULTIPLAYER_URL ?? "",
            ),
          },
          plugins: [
            {
              name: "external-game",
              enforce: "pre",
              resolveId(id) {
                if (id === "./game") return { id: "sloppy:game", external: true };
                if (id === "./net/client") return { id: "sloppy:multiplayer", external: true };
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
        // The startup script imports the game only after the menu paints, and the
        // game's own imports would be discovered only after it downloads. Fetch
        // the whole static graph now, in parallel with the physics binary.
        const preloads = new Set<string>();
        const collect = (fileName: string) => {
          const chunk = context.bundle?.[fileName];
          if (chunk?.type !== "chunk" || preloads.has(fileName)) return;
          preloads.add(fileName);
          chunk.imports.forEach(collect);
        };
        if (gameChunk) collect(gameChunk.fileName);
        const links = preloads.size
          ? `<script>if(!new URLSearchParams(location.search).has("room")&&!new URLSearchParams(location.search).has("multiplayer")){for(const href of ${JSON.stringify([...preloads].map((file) => base + file))}){const link=document.createElement("link");link.rel="modulepreload";link.crossOrigin="anonymous";link.href=href;document.head.append(link);}}</script>`
          : "";
        return html.replace("</head>", `${links}${style}</head>`).replace(
          "<!-- startup-script -->",
          `<script type="module">${code
            .replace(/import\((["'`])sloppy:game\1\)/g, entryImport(gameUrl, gameChunk?.fileName))
            .replace(
              /import\((["'`])sloppy:multiplayer\1\)/g,
              entryImport(multiplayerUrl, multiplayerChunk?.fileName),
            )
            .replace(/<\/script/gi, "<\\/script")}</script>`,
        );
      },
    },
  };
}
