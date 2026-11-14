import { readdir, readFile, writeFile } from "node:fs/promises";
import { promisify } from "node:util";
import { brotliCompress, gzip, constants } from "node:zlib";

// Quality 11 makes the shell's binary 4% (82 KB) smaller than 10 does, for about six
// more seconds of the deploy; a browser decodes either as fast.
const BROTLI_QUALITY = 11;
// Brotli looks back over a window of 4 MB unless told otherwise (window bits 22), and
// the shell's binary is larger, so its far repeats went unfound: a window that holds the
// whole file makes it 2 KB smaller. 24 bits (16 MB) is the most a browser decodes.
const DEFAULT_WINDOW_BITS = 22;
const MAX_WINDOW_BITS = 24;
const windowBits = (size) =>
  Math.min(MAX_WINDOW_BITS, Math.max(DEFAULT_WINDOW_BITS, Math.ceil(Math.log2(size + 16))));

const brotliCompressAsync = promisify(brotliCompress);
const gzipAsync = promisify(gzip);
// The build to prepare, relative to the repository: dist-dev or dist.
const build = process.argv[2];
if (!build) {
  throw new Error("Usage: bun deploy/site/precompress.mjs <build directory>");
}
const assetDirectory = new URL(`../../${build}/assets/`, import.meta.url);
const wasmFiles = (await readdir(assetDirectory)).filter((name) => name.endsWith(".wasm"));
// The shader tables (src/generated/shaders/).
const tableFiles = (await readdir(assetDirectory)).filter((name) => name.endsWith(".json"));
const fontFiles = (await readdir(new URL("fonts/", assetDirectory)))
  .filter((name) => name.endsWith(".ttc"))
  .map((name) => `fonts/${name}`);

if (wasmFiles.length === 0 || fontFiles.length === 0) {
  throw new Error(`Build ${build} before preparing Worker assets`);
}

// The files compress at once on zlib's thread pool, so the wait is the largest one's.
await Promise.all(
  [...wasmFiles, ...tableFiles, ...fontFiles].map(async (file) => {
    const original = await readFile(new URL(file, assetDirectory));
    const [brotli, gzipped] = await Promise.all([
      brotliCompressAsync(original, {
        params: {
          [constants.BROTLI_PARAM_QUALITY]: BROTLI_QUALITY,
          [constants.BROTLI_PARAM_LGWIN]: windowBits(original.length),
        },
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
