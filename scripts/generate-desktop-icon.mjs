// The Mac app's icon, derived from the web app's existing tank artwork. Run on a
// Mac after changing the source art; packaging copies the checked-in .icns file.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const source = new URL("../public/icons/icon-512.png", import.meta.url);
const icon = new URL("../public/icons/mac-app.icns", import.meta.url);
const provenance = new URL("../assets/texture-sources/mac-app-icon.json", import.meta.url);
// Apple's icon grid, unchanged from the original Rust generator.
const CANVAS = 1024;
const BODY = 824;
const RADIUS = 185.4;
const SHADOW_DROP = 10;
const SHADOW_BLUR = 12;
const SHADOW_ALPHA = 0.35;
const SIZES = [16, 32, 128, 256, 512];

function hash(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function fingerprints() {
  return {
    sourceSha256: hash(readFileSync(source)),
    generatorSha256: hash(readFileSync(new URL(import.meta.url), "utf8").replaceAll("\r\n", "\n")),
    iconSha256: hash(readFileSync(icon)),
  };
}

/** Refuse stale artwork rather than silently shipping the previous icon. Uses only
 * Node built-ins, so packaging needs no image library or dependency install. */
export function verifyDesktopIcon() {
  const expected = JSON.parse(readFileSync(provenance, "utf8"));
  const actual = fingerprints();
  if (Object.keys(actual).some((key) => actual[key] !== expected[key])) {
    throw new Error("The Mac icon is stale. Run pnpm run generate:desktop-icon on a Mac.");
  }
  return fileURLToPath(icon);
}

async function generate() {
  if (process.platform !== "darwin") {
    throw new Error("Generating the Mac icon needs macOS's iconutil.");
  }
  // The existing artwork dependency is needed only when regenerating the asset.
  const { createCanvas, loadImage } = await import("@napi-rs/canvas");
  const image = await loadImage(fileURLToPath(source));
  const body = createCanvas(BODY, BODY);
  const bodyContext = body.getContext("2d");
  bodyContext.imageSmoothingQuality = "high";
  bodyContext.beginPath();
  bodyContext.roundRect(0, 0, BODY, BODY, RADIUS);
  bodyContext.clip();
  bodyContext.drawImage(image, 0, 0, BODY, BODY);

  const canvas = createCanvas(CANVAS, CANVAS);
  const context = canvas.getContext("2d");
  context.shadowColor = `rgba(0, 0, 0, ${SHADOW_ALPHA})`;
  // Canvas shadowBlur uses half its value as the Gaussian standard deviation.
  context.shadowBlur = SHADOW_BLUR * 2;
  context.shadowOffsetY = SHADOW_DROP;
  const inset = (CANVAS - BODY) / 2;
  context.drawImage(body, inset, inset);

  const iconset = new URL("../target/desktop-icon/AppIcon.iconset/", import.meta.url);
  rmSync(iconset, { recursive: true, force: true });
  mkdirSync(iconset, { recursive: true });
  for (const size of SIZES) {
    for (const scale of [1, 2]) {
      const pixels = size * scale;
      const output = createCanvas(pixels, pixels);
      const scaled = output.getContext("2d");
      scaled.imageSmoothingQuality = "high";
      scaled.drawImage(canvas, 0, 0, pixels, pixels);
      const name = `icon_${size}x${size}${scale === 2 ? "@2x" : ""}.png`;
      writeFileSync(new URL(name, iconset), output.toBuffer("image/png"));
    }
  }
  const result = spawnSync(
    "iconutil",
    ["--convert", "icns", "--output", fileURLToPath(icon), fileURLToPath(iconset)],
    { stdio: "inherit" },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`iconutil exited with ${result.status}`);
  writeFileSync(provenance, JSON.stringify(fingerprints(), null, 2) + "\n");
  console.log(`Generated ${fileURLToPath(icon)}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await generate();
