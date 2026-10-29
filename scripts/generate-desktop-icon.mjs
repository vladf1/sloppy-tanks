// The Mac app's icon, derived from the web app's existing tank artwork. Generation
// works on any OS; packaging copies the checked-in .icns file.
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const source = new URL("../public/icons/icon-512.png", import.meta.url);
const icon = new URL("../assets/desktop/mac-app.icns", import.meta.url);
const provenance = new URL("../assets/desktop/mac-app-icon.json", import.meta.url);
// Apple's icon grid. Bump version when changing the drawing/scaling algorithm;
// wording, formatting and verification changes do not change the artwork.
const RECIPE = {
  version: 2,
  canvas: 1024,
  body: 824,
  radius: 185.4,
  shadowDrop: 10,
  shadowBlur: 12,
  shadowAlpha: 0.35,
  entries: [
    ["icp4", 16],
    ["ic11", 32],
    ["icp5", 32],
    ["ic12", 64],
    ["ic07", 128],
    ["ic13", 256],
    ["ic08", 256],
    ["ic14", 512],
    ["ic09", 512],
    ["ic10", 1024],
  ],
};

function hash(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function fingerprints(source, icon) {
  return {
    sourceSha256: hash(readFileSync(source)),
    recipeSha256: hash(JSON.stringify(RECIPE)),
    iconSha256: hash(readFileSync(icon)),
  };
}

/** Refuse stale artwork rather than silently shipping the previous icon. Uses only
 * Node built-ins, so packaging needs no image library or dependency install. */
export function verifyDesktopIcon(paths = {}) {
  const {
    source: sourcePath = source,
    icon: iconPath = icon,
    provenance: provenancePath = provenance,
  } = paths;
  const expected = JSON.parse(readFileSync(provenancePath, "utf8"));
  const actual = fingerprints(sourcePath, iconPath);
  const mismatches = Object.keys(actual).filter((key) => actual[key] !== expected[key]);
  if (mismatches.length) {
    throw new Error(
      `The Mac icon differs: ${mismatches.join(", ")}. Run bun run generate:desktop-icon and commit the icon and provenance.`,
    );
  }
  return fileURLToPath(iconPath);
}

export async function generateDesktopIcon(
  destination = new URL("../assets/desktop/", import.meta.url),
) {
  const icon = new URL("mac-app.icns", destination);
  const provenance = new URL("mac-app-icon.json", destination);
  // The existing artwork dependency is needed only when regenerating the asset.
  const { createCanvas, loadImage } = await import("@napi-rs/canvas");
  const image = await loadImage(fileURLToPath(source));
  const { canvas: size, body: bodySize, radius, shadowDrop, shadowBlur, shadowAlpha } = RECIPE;
  const body = createCanvas(bodySize, bodySize);
  const bodyContext = body.getContext("2d");
  bodyContext.imageSmoothingQuality = "high";
  bodyContext.beginPath();
  bodyContext.roundRect(0, 0, bodySize, bodySize, radius);
  bodyContext.clip();
  bodyContext.drawImage(image, 0, 0, bodySize, bodySize);

  let canvas = createCanvas(size, size);
  const context = canvas.getContext("2d");
  context.shadowColor = `rgba(0, 0, 0, ${shadowAlpha})`;
  // Canvas shadowBlur uses half its value as the Gaussian standard deviation.
  context.shadowBlur = shadowBlur * 2;
  context.shadowOffsetY = shadowDrop;
  const inset = (size - bodySize) / 2;
  context.drawImage(body, inset, inset);

  // Halve at each step so small icons average all source pixels instead of
  // sampling a few distant ones in a single 32:1 or 64:1 reduction.
  const images = new Map([[size, canvas.toBuffer("image/png")]]);
  while (canvas.width > 16) {
    const pixels = canvas.width / 2;
    const output = createCanvas(pixels, pixels);
    const scaled = output.getContext("2d");
    scaled.imageSmoothingQuality = "high";
    scaled.drawImage(canvas, 0, 0, pixels, pixels);
    images.set(pixels, output.toBuffer("image/png"));
    canvas = output;
  }
  // Modern ICNS entries are PNGs with an eight-byte type/length header. Keeping
  // the container in Node avoids macOS-only iconutil during asset generation.
  const chunks = RECIPE.entries.map(([type, pixels]) => {
    const png = images.get(pixels);
    const header = Buffer.alloc(8);
    header.write(type, 0, "ascii");
    header.writeUInt32BE(8 + png.length, 4);
    return Buffer.concat([header, png]);
  });
  const header = Buffer.alloc(8);
  header.write("icns", 0, "ascii");
  header.writeUInt32BE(8 + chunks.reduce((sum, chunk) => sum + chunk.length, 0), 4);
  mkdirSync(new URL(".", icon), { recursive: true });
  writeFileSync(icon, Buffer.concat([header, ...chunks]));
  writeFileSync(provenance, JSON.stringify(fingerprints(source, icon), null, 2) + "\n");
  console.log(`Generated ${fileURLToPath(icon)}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await generateDesktopIcon();
