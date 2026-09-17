import { createCanvas, loadImage } from "@napi-rs/canvas";
import { writeFile } from "node:fs/promises";

const kinds = ["scout", "balanced", "heavy"];

/** Three vehicle columns, with one row per team. */
export async function packTankPreviews(assets) {
  const canvas = createCanvas(1920, 800);
  const context = canvas.getContext("2d");
  for (const team of [0, 1]) {
    for (const [column, kind] of kinds.entries()) {
      const name = `${team}-${kind}`;
      const source = Buffer.from(assets[name].split(",")[1], "base64");
      const image = await loadImage(source);
      if (image.width !== 640 || image.height !== 400) {
        throw new Error(`Unexpected preview dimensions: ${name}`);
      }
      context.drawImage(image, column * 640, team * 400);
    }
  }
  const image = await canvas.encode("webp", 88);
  await writeFile("public/previews/tanks.webp", image);
  console.log(`public/previews/tanks.webp: ${image.length} bytes`);
}
