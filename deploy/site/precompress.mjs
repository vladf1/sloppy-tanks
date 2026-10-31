import { readdir, readFile, writeFile } from "node:fs/promises";
import { promisify } from "node:util";
import { brotliCompress, gzip, constants } from "node:zlib";

// Quality 11 is about 4% smaller than 10 for the engines but takes twice as long.
const BROTLI_QUALITY = 10;

const brotliCompressAsync = promisify(brotliCompress);
const gzipAsync = promisify(gzip);
// The build to prepare, relative to the repository: dist-dev or dist.
const build = process.argv[2];
if (!build) {
  throw new Error("Usage: node deploy/site/precompress.mjs <build directory>");
}
const assetDirectory = new URL(`../../${build}/assets/`, import.meta.url);
const wasmFiles = (await readdir(assetDirectory)).filter((name) => name.endsWith(".wasm"));

if (wasmFiles.length === 0) {
  throw new Error(`Build ${build} before preparing Worker assets`);
}

// The engines compress at once on zlib's thread pool, so the wait is the largest file's.
await Promise.all(
  wasmFiles.map(async (file) => {
    const original = await readFile(new URL(file, assetDirectory));
    const [brotli, gzipped] = await Promise.all([
      brotliCompressAsync(original, {
        params: { [constants.BROTLI_PARAM_QUALITY]: BROTLI_QUALITY },
      }),
      gzipAsync(original, { level: 9 }),
    ]);

    await writeFile(new URL(`${file}.br`, assetDirectory), brotli);
    await writeFile(new URL(`${file}.gz`, assetDirectory), gzipped);
    console.log(
      `assets/${file}: ${original.length} bytes -> ${brotli.length} Brotli, ${gzipped.length} gzip`,
    );
  }),
);
