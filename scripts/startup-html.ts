import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { build, minify, type Plugin } from "vite";
import { mapChoiceMarkup } from "./map-picker-markup.ts";
import { buildLabel, pageBuild } from "./page-health.ts";

const entry = fileURLToPath(new URL("../src/main.ts", import.meta.url));
/** The modules the startup script imports lazily: each is a build entry of its own, and an
 * external `id` in the inline build. The game comes first. */
const lazyEntries = [
  { source: "src/game.ts", id: "sloppy:game" },
  { source: "src/net/client.ts", id: "sloppy:multiplayer" },
  { source: "src/net/room-browser.ts", id: "sloppy:rooms" },
].map((lazy) => ({ ...lazy, file: fileURLToPath(new URL(`../${lazy.source}`, import.meta.url)) }));

/**
 * A parser-blocking script right after Battle Setup that opens the tab a
 * `?multiplayer` or room link asks for. The startup module is deferred, so without
 * this the browser can paint the markup's single-player tab first. It mirrors
 * `initialPlayMode` and `showPlayMode` in `src/game/play-modes.ts`, which then
 * apply the same state again.
 */
function initialTabScript(): string {
  const configured = JSON.stringify(Boolean(process.env.VITE_MULTIPLAYER_URL));
  return `<script>(()=>{const p=new URLSearchParams(location.search);if(!(${configured}||["localhost","127.0.0.1"].includes(location.hostname))||!(p.has("multiplayer")||p.has("room")))return;const s=document.querySelector("#startup-overlay .start");if(!s)return;s.dataset.play="multiplayer";for(const t of s.querySelectorAll('[role="tab"][data-play]')){const on=t.dataset.play==="multiplayer";t.setAttribute("aria-selected",String(on));t.tabIndex=on?0:-1;const panel=document.getElementById(t.getAttribute("aria-controls"));if(panel)panel.hidden=!on}})()</script>`;
}

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
      for (const { file } of lazyEntries) {
        this.emitFile({ type: "chunk", id: file, preserveSignature: "strict" });
      }
    },
    transformIndexHtml: {
      order: "post",
      async handler(html, context) {
        if (html.includes("<!-- battle-setup -->")) {
          const markup = await readFile(
            new URL("../src/game/battle-setup.html", import.meta.url),
            "utf8",
          );
          const setup = markup
            .replaceAll("%BASE_URL%", base)
            .replace("%BUILD_LABEL%", buildLabel(pageBuild()))
            .replace(
              /<!-- map-choice:(\w+):([\w-]+) -->/g,
              (_comment, name: string, label: string) => mapChoiceMarkup(name, label),
            );
          // Replacer functions insert text literally; a replacement string would read `$&`.
          html = html.replace(
            "<!-- battle-setup -->",
            () =>
              `<div id="startup-overlay" data-state="loading">${setup}</div>${initialTabScript()}`,
          );
        }
        if (!html.includes("<!-- startup-script -->")) return html;
        const chunks = lazyEntries.map(({ source, file }) => {
          const chunk = Object.values(context.bundle ?? {}).find(
            (item) => item.type === "chunk" && item.facadeModuleId === file,
          );
          if (context.bundle && !chunk) throw new Error(`Missing ${source} entry chunk`);
          return chunk;
        });
        const [gameChunk] = chunks;
        /** The chunks statically reachable from `fileName`, in visiting order, and their
         * stylesheets, each after the stylesheets it depends on. */
        const staticGraph = (fileName?: string) => {
          const files = new Set<string>();
          const styles = new Set<string>();
          const visit = (file: string) => {
            const chunk = context.bundle?.[file];
            if (chunk?.type !== "chunk" || files.has(file)) return;
            files.add(file);
            chunk.imports.forEach(visit);
            chunk.viteMetadata?.importedCss.forEach((css) => styles.add(base + css));
          };
          if (fileName) visit(fileName);
          return { files, styles };
        };
        // These entries are external to the inline build, so Vite cannot attach
        // its usual dynamic-import CSS loader. Load each entry's static CSS
        // graph before starting it, and only when that mode is selected.
        const entryImport = (url: string, fileName?: string) => {
          const { styles } = staticGraph(fileName);
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
              resolveId(id, importer) {
                const target = importer && id.startsWith(".") ? resolve(dirname(importer), id) : id;
                const lazy = lazyEntries.find(({ file }) => file === `${target}.ts`);
                return lazy ? { id: lazy.id, external: true } : null;
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
        // the whole static graph now, in parallel with the engine binary.
        const preloads = staticGraph(gameChunk?.fileName).files;
        const links = preloads.size
          ? `<script>{const p=new URLSearchParams(location.search);if(!p.has("room")&&!p.has("multiplayer")){for(const href of ${JSON.stringify([...preloads].map((file) => base + file))}){const link=document.createElement("link");link.rel="modulepreload";link.crossOrigin="anonymous";link.href=href;document.head.append(link);}}}</script>`
          : "";
        // The dev server has no bundle; it serves each entry from its source.
        const imports = new Map(
          lazyEntries.map(({ id, source }, i) => {
            const fileName = chunks[i]?.fileName;
            return [id, entryImport(`${base}${fileName ?? source}`, fileName)];
          }),
        );
        // Minified code can contain `$&`, which a replacement string would expand.
        const startup = code.replace(
          /import\((["'`])(sloppy:\w+)\1\)/g,
          (load, _quote, id: string) => imports.get(id) ?? load,
        );
        return html
          .replace("</head>", () => `${links}${style}</head>`)
          .replace(
            "<!-- startup-script -->",
            () => `<script type="module">${startup.replace(/<\/script/gi, "<\\/script")}</script>`,
          );
      },
    },
  };
}
