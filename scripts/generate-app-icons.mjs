// Draws the Home Screen icons in public/icons/: the favicon's blue Bruiser
// (public/favicon.svg, `bun run generate:favicon`) on the menu's navy, rendered by Chrome.
// Home screens mask the square themselves and fill transparency with black, so each icon
// is an opaque full square. The page's own icons (Apple's and the manifest's) carry a gold
// bookmark ribbon, so a Home Screen bookmark of the site reads apart from the iOS and
// desktop apps, whose icons come from the plain ones.
import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { chromium } from "playwright";
import { headless } from "./browser-helpers.mjs";
import { generateDesktopIcon } from "./generate-desktop-icon.mjs";

/** Apple's Home Screen size and the manifest's two standard sizes, with the ribbon; the
 * apps' plain icons (`generate-desktop-icon.mjs`, `build-desktop.mjs`, the window icon). */
const ICONS = [
  { file: "apple-touch-icon.png", size: 180, bookmark: true },
  { file: "web-icon-192.png", size: 192, bookmark: true },
  { file: "web-icon-512.png", size: 512, bookmark: true },
  { file: "icon-192.png", size: 192, bookmark: false },
  { file: "icon-512.png", size: 512, bookmark: false },
];
/** The tank's share of the icon's width, clear of the rounded corners. */
const TANK_SCALE = 0.74;
/** A bookmark ribbon hanging from the top edge at the right, in percent of the icon: clear
 * of the turret and inside a Home Screen's rounded mask, in the menu's gold
 * (`theme::GOLD_HOT`, `GOLD`, `GOLD_DEEP`) with the tank's dark outline. */
const RIBBON = `<svg viewBox="0 0 100 100" width="100%" height="100%"
style="position:absolute;inset:0;filter:drop-shadow(0 0.6% 0.8% rgba(0,0,0,0.5))">
<defs><linearGradient id="ribbon" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="#ffe58a"/><stop offset="0.35" stop-color="#ffcf38"/><stop offset="1" stop-color="#ff9d1c"/>
</linearGradient></defs>
<path d="M66 -3 H83 V31 L74.5 24.5 L66 31 Z" fill="url(#ribbon)" stroke="#0d1b2a" stroke-width="2.2" stroke-linejoin="round"/>
</svg>`;

const favicon = await readFile(new URL("../public/favicon.svg", import.meta.url), "utf8");
const page = (size, bookmark) => `<!doctype html><html><body style="margin:0">
<div style="position:relative;width:${size}px;height:${size}px;background:radial-gradient(circle at 50% 42%, #2b6189 0%, #1b3c5f 48%, #0d1b2a 100%)">
<div style="position:absolute;left:50%;top:52%;width:${size * TANK_SCALE}px;height:${size * TANK_SCALE}px;transform:translate(-50%,-50%)">${favicon.replace("<svg ", '<svg width="100%" height="100%" ')}</div>
${bookmark ? RIBBON : ""}
</div></body></html>`;

const browser = await chromium.launch({ channel: "chrome", headless });
try {
  for (const { file, size, bookmark } of ICONS) {
    const context = await browser.newContext({
      viewport: { width: size, height: size },
      deviceScaleFactor: 1,
    });
    const view = await context.newPage();
    await view.setContent(page(size, bookmark));
    await view.screenshot({
      path: new URL(`../public/icons/${file}`, import.meta.url).pathname,
      clip: { x: 0, y: 0, width: size, height: size },
    });
    await context.close();
    console.log(`public/icons/${file} (${size}×${size})`);
  }
} finally {
  await browser.close();
}
// Chrome's screenshots are stored loosely; oxipng recompresses them losslessly (the
// 512 icon from about 103 KB to 60), pixel for pixel the same.
execFileSync(
  "oxipng",
  ["-o", "max", "--zopfli", "--strip", "safe", ...ICONS.map(({ file }) => `public/icons/${file}`)],
  { cwd: new URL("..", import.meta.url).pathname, stdio: "inherit" },
);
await generateDesktopIcon();
