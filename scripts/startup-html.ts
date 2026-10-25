import { readFile } from "node:fs/promises";
import { mapChoiceMarkup } from "./map-picker-markup.ts";
import { buildLabel, pageBuild } from "./page-health.ts";

export const lazyEntries = [
  { source: "src/game.ts", id: "sloppy:game" },
  { source: "src/net/client.ts", id: "sloppy:multiplayer" },
  { source: "src/net/room-browser.ts", id: "sloppy:rooms" },
];

/** Start the one engine download before any deferred module runs. The loader takes it over. */
export function engineDownloadScript(webgpuUrl: string, webglUrl: string): string {
  return [
    "(()=>{",
    "const g=/[?&]webgl(=|&|$)/.test(location.search)?null:navigator.gpu;",
    "const a=new AbortController();",
    `const w=g&&fetch(${JSON.stringify(webgpuUrl)},{signal:a.signal});`,
    'const api=g?g.requestAdapter().then(d=>d?"webgpu":"webgl",()=>"webgl"):Promise.resolve("webgl");',
    `const b=api.then(k=>k==="webgpu"?w:(a.abort(),fetch(${JSON.stringify(webglUrl)})));`,
    "w&&w.catch(()=>{});b.catch(()=>{});",
    "window.sloppyGraphics=api;window.sloppyEngineBinary=b;",
    "})();",
  ].join("");
}

/** Parser-blocking: room links must never paint the single-player tab first. */
function initialTabScript(multiplayerUrl: string): string {
  const configured = JSON.stringify(Boolean(multiplayerUrl));
  return `<script>(()=>{const p=new URLSearchParams(location.search);if(!(${configured}||["localhost","127.0.0.1"].includes(location.hostname))||!(p.has("multiplayer")||p.has("room")))return;const s=document.querySelector("#startup-overlay .start");if(!s)return;s.dataset.play="multiplayer";for(const t of s.querySelectorAll('[role="tab"][data-play]')){const on=t.dataset.play==="multiplayer";t.setAttribute("aria-selected",String(on));t.tabIndex=on?0:-1;const panel=document.getElementById(t.getAttribute("aria-controls"));if(panel)panel.hidden=!on}})()</script>`;
}

export async function startupHtml(
  html: string,
  base: string,
  multiplayerUrl: string,
  script: string,
  css: string,
  preloads: string[],
  binaries: [string, string],
): Promise<string> {
  const markup = await readFile(new URL("../src/game/battle-setup.html", import.meta.url), "utf8");
  const setup = markup
    .replaceAll("%BASE_URL%", base)
    .replace("%BUILD_LABEL%", buildLabel(pageBuild()))
    .replace(/<!-- map-choice:(\w+):([\w-]+) -->/g, (_comment, name: string, label: string) =>
      mapChoiceMarkup(name, label),
    );
  const links = `<script>{const p=new URLSearchParams(location.search);if(!p.has("room")&&!p.has("multiplayer")){for(const href of ${JSON.stringify(preloads)}){const link=document.createElement("link");link.rel="modulepreload";link.crossOrigin="anonymous";link.href=href;document.head.append(link);}}}</script>`;
  return html
    .replace(
      "<!-- battle-setup -->",
      () =>
        `<div id="startup-overlay" data-state="loading">${setup}</div>${initialTabScript(multiplayerUrl)}`,
    )
    .replace(
      "</head>",
      () =>
        `<script>${engineDownloadScript(...binaries)}</script>${links}<style>${css.replace(/<\/style/gi, "<\\/style")}</style></head>`,
    )
    .replace(
      "<!-- startup-script -->",
      () => `<script type="module">${script.replace(/<\/script/gi, "<\\/script")}</script>`,
    );
}
