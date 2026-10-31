import { readdir, readFile, writeFile } from "node:fs/promises";
import { promisify } from "node:util";
import { brotliCompress, gzip, constants } from "node:zlib";

// Quality 11 is about 4% smaller than 10 for the engines but takes twice as long.
const BROTLI_QUALITY = 10;

const brotliCompressAsync = promisify(brotliCompress);
const gzipAsync = promisify(gzip);
const assetDirectory = new URL("../../dist-dev/assets/", import.meta.url);
const wasmFiles = (await readdir(assetDirectory)).filter((name) => name.endsWith(".wasm"));
const fontFiles = (await readdir(new URL("fonts/", assetDirectory)))
  .filter((name) => name.endsWith(".ttf"))
  .map((name) => `fonts/${name}`);

if (wasmFiles.length === 0 || fontFiles.length === 0) {
  throw new Error("Build dist-dev before preparing Worker assets");
}

// The files compress at once on zlib's thread pool, so the wait is the largest one's.
await Promise.all(
  [...wasmFiles, ...fontFiles].map(async (file) => {
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
