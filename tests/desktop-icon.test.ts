import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { verifyDesktopIcon } from "../scripts/generate-desktop-icon.mjs";

test("desktop packaging can verify its icon without loading native image libraries", () => {
  execFileSync(
    process.execPath,
    [
      "--no-addons",
      "--input-type=module",
      "--eval",
      "import { verifyDesktopIcon } from './scripts/generate-desktop-icon.mjs'; verifyDesktopIcon();",
    ],
    { cwd: fileURLToPath(new URL("..", import.meta.url)) },
  );
});

test("the bundled Mac icon matches its source art and generator", () => {
  const icon = readFileSync(verifyDesktopIcon());
  assert.equal(icon.toString("ascii", 0, 4), "icns");
  assert.equal(icon.readUInt32BE(4), icon.length);
  const sizes = new Set<number>();
  for (let offset = 8; offset < icon.length;) {
    const length = icon.readUInt32BE(offset + 4);
    assert.ok(length >= 8 && offset + length <= icon.length);
    // iconutil stores the 16-pixel icon as ARGB; larger entries contain PNGs.
    if (icon.toString("ascii", offset, offset + 4) === "ic04") sizes.add(16);
    if (
      icon.subarray(offset + 8, offset + 16).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))
    ) {
      sizes.add(icon.readUInt32BE(offset + 24));
      assert.equal(icon.readUInt32BE(offset + 24), icon.readUInt32BE(offset + 28));
    }
    offset += length;
  }
  assert.deepEqual(
    [...sizes].sort((a, b) => a - b),
    [16, 32, 64, 128, 256, 512, 1024],
  );
});
