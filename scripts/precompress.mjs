import { readdir, readFile, writeFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { promisify } from "node:util";
import { brotliCompress, constants, gzip } from "node:zlib";

const TEXT_BROTLI_QUALITY = 11;
// Quality 11 is about 4% smaller than 10 for the engines but takes twice as long.
const WASM_BROTLI_QUALITY = 10;
const GZIP_LEVEL = 9;

const brotliCompressAsync = promisify(brotliCompress);
const gzipAsync = promisify(gzip);

/** Writes Brotli and gzip copies (`X.br`, `X.gz`) beside every file under `directory`
 * that `select(path, bytes)` picks, for hosts that send the copy a client accepts instead
 * of compressing on each request. A copy no smaller than its file is not written. Returns
 * each picked file's path relative to `directory` and its sizes. */
export async function precompress(directory, select) {
  const entries = await readdir(directory, { recursive: true, withFileTypes: true });
  const paths = entries
    .filter((entry) => entry.isFile())
    .map((entry) => relative(directory, join(entry.parentPath, entry.name)));
  const picked = [];
  for (const path of paths) {
    const bytes = await readFile(join(directory, path));
    if (select(path, bytes)) picked.push({ path, bytes });
  }
  // Every file compresses at once on zlib's thread pool.
  return Promise.all(
    picked.map(async ({ path, bytes }) => {
      const quality = path.endsWith(".wasm") ? WASM_BROTLI_QUALITY : TEXT_BROTLI_QUALITY;
      const [brotli, gzipped] = await Promise.all([
        brotliCompressAsync(bytes, { params: { [constants.BROTLI_PARAM_QUALITY]: quality } }),
        gzipAsync(bytes, { level: GZIP_LEVEL }),
      ]);
      const sizes = { path, original: bytes.length, brotli: brotli.length, gzip: gzipped.length };
      if (brotli.length < bytes.length) await writeFile(join(directory, `${path}.br`), brotli);
      if (gzipped.length < bytes.length) await writeFile(join(directory, `${path}.gz`), gzipped);
      return sizes;
    }),
  );
}
