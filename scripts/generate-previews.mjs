import { chromium } from "playwright";
import { createServer } from "vite";
import { mkdir } from "node:fs/promises";
import { packTankPreviews } from "./pack-tank-previews.mjs";

const server = await createServer({ server: { host: "127.0.0.1", port: 0 } });
await server.listen();
const browser = await chromium.launch({ channel: "chrome", headless: true });
try {
  const page = await browser.newPage();
  await page.goto(`${server.resolvedUrls.local[0]}scripts/tank-preview-assets.html`);
  await page.waitForFunction(() => !!window.tankPreviewAssets);
  const assets = await page.evaluate(() => window.tankPreviewAssets);
  await mkdir("public/previews", { recursive: true });
  await packTankPreviews(assets);
} finally {
  await browser.close();
  await server.close();
}
