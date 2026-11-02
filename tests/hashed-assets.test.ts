import assert from "node:assert/strict";
import { test } from "bun:test";
import { hashedName, runtimeFiles } from "../scripts/hashed-assets";

test("the page's runtime files are the shell's and the renderer's, not the licences", () => {
  const files = runtimeFiles();
  for (const path of [
    "fonts/interface.ttc",
    "previews/tanks.webp",
    "audio/shot.mp3",
    "textures/houses/brick.webp",
  ]) {
    assert.ok(files.includes(path), path);
  }
  assert.ok(!files.some((path) => /LICENSE|\.md$|\.txt$/.test(path)));
});

test("a hashed name keeps the path and changes with the content", () => {
  const name = hashedName("textures/houses/brick.webp", new Uint8Array([1, 2, 3]));
  assert.match(name, /^assets\/textures\/houses\/brick-[\w-]{8}\.webp$/);
  assert.equal(hashedName("textures/houses/brick.webp", new Uint8Array([1, 2, 3])), name);
  assert.notEqual(hashedName("textures/houses/brick.webp", new Uint8Array([1, 2, 4])), name);
});
