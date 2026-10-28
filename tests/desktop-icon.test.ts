import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";
import { generateDesktopIcon, verifyDesktopIcon } from "../scripts/generate-desktop-icon.mjs";

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

function iconEntries(path = verifyDesktopIcon()) {
  const icon = readFileSync(path);
  assert.equal(icon.toString("ascii", 0, 4), "icns");
  assert.equal(icon.readUInt32BE(4), icon.length);
  const entries = new Map<string, Buffer>();
  for (let offset = 8; offset < icon.length;) {
    assert.ok(offset + 8 <= icon.length);
    const length = icon.readUInt32BE(offset + 4);
    assert.ok(length >= 8 && offset + length <= icon.length);
    const type = icon.toString("ascii", offset, offset + 4);
    assert.ok(!entries.has(type), `duplicate entry: ${type}`);
    entries.set(type, icon.subarray(offset + 8, offset + length));
    offset += length;
  }
  return entries;
}

test("Mac icon generation runs without platform commands and produces verifiable assets", async (t) => {
  const directory = mkdtempSync(join(tmpdir(), "sloppy-desktop-icon-generate-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const destination = pathToFileURL(directory + "/");
  await generateDesktopIcon(destination);
  const path = verifyDesktopIcon({
    icon: new URL("mac-app.icns", destination),
    provenance: new URL("mac-app-icon.json", destination),
  });
  assert.deepEqual([...iconEntries(path).keys()].sort(), [...iconEntries().keys()].sort());
});

test("the bundled Mac icon contains every 1x and 2x slot, including shared pixel sizes", () => {
  const entries = iconEntries();
  const expected = {
    icp4: 16,
    ic11: 32,
    icp5: 32,
    ic12: 64,
    ic07: 128,
    ic13: 256,
    ic08: 256,
    ic14: 512,
    ic09: 512,
    ic10: 1024,
  };
  assert.deepEqual([...entries.keys()].sort(), Object.keys(expected).sort());
  for (const [type, size] of Object.entries(expected)) {
    const png = entries.get(type)!;
    assert.ok(png.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])));
    assert.equal(png.readUInt32BE(16), size, `${type} width`);
    assert.equal(png.readUInt32BE(20), size, `${type} height`);
  }
});

test("small Mac icons preserve corner and shadow coverage when downscaled", async () => {
  const { createCanvas, loadImage } = await import("@napi-rs/canvas");
  const entries = iconEntries();
  async function pixels(png: Buffer) {
    const image = await loadImage(png);
    const canvas = createCanvas(image.width, image.height);
    const context = canvas.getContext("2d");
    context.drawImage(image, 0, 0);
    return context.getImageData(0, 0, image.width, image.height);
  }
  const full = await pixels(entries.get("ic10")!);
  for (const type of ["ic11", "ic12"]) {
    const small = await pixels(entries.get(type)!);
    const scale = full.width / small.width;
    for (let y = 0; y < small.height; y++) {
      for (let x = 0; x < small.width; x++) {
        // Independent area average: a large reduction must retain subpixel
        // corner/shadow coverage instead of sampling isolated source pixels.
        let alpha = 0;
        for (let dy = 0; dy < scale; dy++) {
          for (let dx = 0; dx < scale; dx++) {
            alpha += full.data[((y * scale + dy) * full.width + x * scale + dx) * 4 + 3];
          }
        }
        const expected = alpha / (scale * scale);
        const actual = small.data[(y * small.width + x) * 4 + 3];
        assert.ok(Math.abs(actual - expected) < 20, `${type} alpha at ${x},${y}`);
      }
    }
  }
});

test("icon provenance tolerates comment edits and identifies each mismatched input", (t) => {
  const directory = mkdtempSync(join(tmpdir(), "sloppy-desktop-icon-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  for (const folder of ["scripts", "public/icons", "assets/desktop"]) {
    mkdirSync(join(directory, folder), { recursive: true });
  }
  for (const file of [
    "public/icons/icon-512.png",
    "assets/desktop/mac-app.icns",
    "assets/desktop/mac-app-icon.json",
  ]) {
    copyFileSync(new URL(`../${file}`, import.meta.url), join(directory, file));
  }
  const script = readFileSync(
    new URL("../scripts/generate-desktop-icon.mjs", import.meta.url),
    "utf8",
  );
  writeFileSync(
    join(directory, "scripts/generate-desktop-icon.mjs"),
    script + "\n// Wording only.\n",
  );
  const verify = () =>
    execFileSync(
      process.execPath,
      [
        "--no-addons",
        "--input-type=module",
        "--eval",
        "import { verifyDesktopIcon } from './scripts/generate-desktop-icon.mjs'; try { verifyDesktopIcon(); } catch (error) { process.stdout.write(error.message); process.exitCode = 1; }",
      ],
      { cwd: directory, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    );
  assert.equal(verify(), "");
  const provenance = join(directory, "assets/desktop/mac-app-icon.json");
  const expected = JSON.parse(readFileSync(provenance, "utf8"));
  for (const key of ["sourceSha256", "recipeSha256", "iconSha256"]) {
    writeFileSync(provenance, JSON.stringify({ ...expected, [key]: "mismatch" }));
    assert.throws(verify, (error: unknown) => {
      const { stdout } = error as { stdout: string };
      assert.match(stdout, new RegExp(`differs: ${key}\\.`));
      assert.ok(!stdout.includes("on a Mac"));
      return true;
    });
  }
});
