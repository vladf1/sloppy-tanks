import { fileURLToPath } from "node:url";
import { precompress } from "../../scripts/precompress.mjs";

// Cloudflare compresses the engine binaries on every request, so the Worker sends these
// stored copies instead (worker.mjs).
const assetDirectory = fileURLToPath(new URL("../../dist-dev/assets", import.meta.url));
const sizes = await precompress(assetDirectory, (path) => path.endsWith(".wasm"));

if (sizes.length === 0) {
  throw new Error("Build dist-dev before preparing Worker assets");
}
for (const { path, original, brotli, gzip } of sizes) {
  console.log(`assets/${path}: ${original} bytes -> ${brotli} Brotli, ${gzip} gzip`);
}
