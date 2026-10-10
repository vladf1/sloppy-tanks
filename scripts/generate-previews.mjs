// Tank selection previews: renders the game's vehicle models with the engine
// (`tools/tank-surface-check.html`, the labs build: run `pnpm run wasm -- --labs`
// first) in headless Chrome and packs them into `public/previews/tanks.webp`, three
// chassis columns with one row per team. `SLOPPY_PREVIEWS_OUT` writes elsewhere, for
// comparing a candidate with the checked-in sheet before replacing it.
import { chromium } from "playwright";
import { createServer } from "vite";
import { createCanvas, loadImage } from "@napi-rs/canvas";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { headless } from "./browser-helpers.mjs";

const kinds = ["scout", "balanced", "heavy"];
const output = process.env.SLOPPY_PREVIEWS_OUT ?? "public/previews/tanks.webp";
const TILE = { width: 640, height: 400 };
/** WebP quality of the packed sheet. */
const QUALITY = 88;

async function packTankPreviews(assets) {
  const canvas = createCanvas(TILE.width * kinds.length, TILE.height * 2);
  const context = canvas.getContext("2d");
  for (const team of [0, 1]) {
    for (const [column, kind] of kinds.entries()) {
      const name = `${team}-${kind}`;
      const image = await loadImage(Buffer.from(assets[name].split(",")[1], "base64"));
      if (image.width !== TILE.width || image.height !== TILE.height) {
        throw new Error(`Unexpected preview dimensions: ${name}`);
      }
      context.drawImage(image, column * TILE.width, team * TILE.height);
    }
  }
  const image = await canvas.encode("webp", QUALITY);
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, image);
  console.log(`${output}: ${image.length} bytes`);
}

const server = await createServer({ server: { host: "127.0.0.1", port: 0 } });
await server.listen();
const browser = await chromium.launch({ channel: "chrome", headless });
try {
  const page = await browser.newPage();
  await page.goto(`${server.resolvedUrls.local[0]}tools/tank-surface-check.html`);
  await page.waitForFunction(() => window.tankPreviewAssets || window.tankPreviewError, null, {
    timeout: 120000,
  });
  const error = await page.evaluate(() => window.tankPreviewError);
  if (error) throw new Error(error);
  await packTankPreviews(await page.evaluate(() => window.tankPreviewAssets));
} finally {
  await browser.close();
  await server.close();
}
