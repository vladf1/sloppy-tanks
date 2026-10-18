import { readdir, readFile, writeFile } from "node:fs/promises";
import { brotliCompressSync, gzipSync, constants } from "node:zlib";

const assetDirectory = new URL("../../dist-dev/assets/", import.meta.url);
const wasmFiles = (await readdir(assetDirectory)).filter((name) => name.endsWith(".wasm"));

if (wasmFiles.length === 0) {
  throw new Error("Build dist-dev before preparing Worker assets");
}

for (const file of wasmFiles) {
  const original = await readFile(new URL(file, assetDirectory));
  const brotli = brotliCompressSync(original, {
    params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
  });
  const gzip = gzipSync(original, { level: 9 });

  await writeFile(new URL(`${file}.br`, assetDirectory), brotli);
  await writeFile(new URL(`${file}.gz`, assetDirectory), gzip);
  console.log(
    `assets/${file}: ${original.length} bytes -> ${brotli.length} Brotli, ${gzip.length} gzip`,
  );
}
