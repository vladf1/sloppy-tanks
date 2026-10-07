import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

const publicFile = (path: string) => new URL(`../public/${path}`, import.meta.url);

/** A PNG's width and height, from its IHDR chunk. */
async function pngSize(path: string): Promise<string> {
  const png = await readFile(publicFile(path));
  return `${png.readUInt32BE(16)}x${png.readUInt32BE(20)}`;
}

test("the manifest opens the game standalone under any base, with icons of its sizes", async () => {
  const manifest = JSON.parse(await readFile(publicFile("manifest.webmanifest"), "utf8")) as {
    start_url: string;
    scope: string;
    display: string;
    icons: { src: string; sizes: string }[];
  };
  // Local builds serve /sloppy-tanks/ and the deployed sites /, so URLs stay relative.
  assert.equal(manifest.start_url, "./");
  assert.equal(manifest.scope, "./");
  assert.equal(manifest.display, "standalone");
  assert.ok(manifest.icons.length > 0);
  for (const icon of manifest.icons) {
    assert.ok(!icon.src.startsWith("/"), `${icon.src} is relative`);
    assert.equal(await pngSize(icon.src), icon.sizes);
  }
});

test("the page links the manifest and Apple's Home Screen icon", async () => {
  const html = await readFile(new URL("../index.html", import.meta.url), "utf8");
  assert.match(html, /<link rel="manifest" href="\/manifest\.webmanifest" \/>/);
  const touchIcon = html.match(/<link rel="apple-touch-icon" href="\/([^"]+)" \/>/)?.[1];
  assert.ok(touchIcon);
  assert.equal(await pngSize(touchIcon), "180x180");
});
