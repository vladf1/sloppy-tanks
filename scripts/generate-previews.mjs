// Tank selection previews: renders the game's vehicle models with the engine
// (`tools/tank-surface-check.html`, the labs build: run `pnpm run wasm -- --labs`
// first) in headless Chrome and packs them into `public/previews/tanks.webp`, three
// chassis columns with one row per team. `SLOPPY_PREVIEWS_OUT` writes elsewhere, for
// comparing a candidate with the checked-in sheet before replacing it.
import { chromium } from "playwright";
import { createServer } from "vite";
import { createCanvas, loadImage } from "@napi-rs/canvas";
import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { headless } from "./browser-helpers.mjs";

const kinds = ["scout", "balanced", "heavy"];
const output = process.env.SLOPPY_PREVIEWS_OUT ?? "public/previews/tanks.webp";
const TILE = { width: 640, height: 400 };
/** WebP quality of the packed sheet. The menu shows a tile at most about 0.8 of its
 * size on a 2x screen, where quality 80 looks the same as 88. */
const QUALITY = 80;

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
  // cwebp rather than the canvas's own encoder: at the same quality it packs the
  // alpha (the soft contact shadows) a quarter smaller and writes no ICC profile.
  const image = execFileSync(
    "cwebp",
    [
      "-quiet",
      "-q",
      String(QUALITY),
      "-m",
      "6",
      "-alpha_q",
      "100",
      "-metadata",
      "none",
      "-o",
      "-",
      "--",
      "-",
    ],
    { input: canvas.toBuffer("image/png"), maxBuffer: 64 * 1024 * 1024 },
  );
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
