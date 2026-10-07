// Draws the Home Screen icons in public/icons/: the favicon's blue Bruiser
// (public/favicon.svg, `pnpm run generate:favicon`) on the menu's navy, rendered by Chrome.
// Home screens mask the square themselves and fill transparency with black, so each icon
// is an opaque full square.
import { readFile } from "node:fs/promises";
import { chromium } from "playwright";
import { headless } from "./browser-helpers.mjs";

/** Apple's Home Screen size, then the manifest's two standard sizes. */
const ICONS = [
  { file: "apple-touch-icon.png", size: 180 },
  { file: "icon-192.png", size: 192 },
  { file: "icon-512.png", size: 512 },
];
/** The tank's share of the icon's width, clear of the rounded corners. */
const TANK_SCALE = 0.74;

const favicon = await readFile(new URL("../public/favicon.svg", import.meta.url), "utf8");
const page = (size) => `<!doctype html><html><body style="margin:0">
<div style="position:relative;width:${size}px;height:${size}px;background:radial-gradient(circle at 50% 42%, #2b6189 0%, #1b3c5f 48%, #0d1b2a 100%)">
<div style="position:absolute;left:50%;top:52%;width:${size * TANK_SCALE}px;height:${size * TANK_SCALE}px;transform:translate(-50%,-50%)">${favicon.replace("<svg ", '<svg width="100%" height="100%" ')}</div>
</div></body></html>`;

const browser = await chromium.launch({ channel: "chrome", headless });
try {
  for (const { file, size } of ICONS) {
    const context = await browser.newContext({
      viewport: { width: size, height: size },
      deviceScaleFactor: 1,
    });
    const view = await context.newPage();
    await view.setContent(page(size));
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
