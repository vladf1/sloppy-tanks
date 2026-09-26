import { createCanvas, loadImage } from "@napi-rs/canvas";
import { writeFile } from "node:fs/promises";
import { encodeWebp } from "./encode-webp";
import {
  PICKUP_ATLAS_PATH,
  PICKUP_ATLAS_PADDING,
  PICKUP_ATLAS_SIZE,
  PICKUP_ATLAS_STRIDE,
  PICKUP_ATLAS_TILES,
  PICKUP_ICON_SIZE,
} from "../src/game/pickup-atlas";

// Pack existing, lossless artwork offline. Extrude edge pixels into every gutter
// so filtering/mipmaps do not sample the neighboring pickup's icon.
const canvas = createCanvas(PICKUP_ATLAS_SIZE, PICKUP_ATLAS_SIZE);
const context = canvas.getContext("2d");
context.imageSmoothingEnabled = false;
for (const [kind, [column, row]] of Object.entries(PICKUP_ATLAS_TILES)) {
  const image = await loadImage(
    new URL(`../assets/texture-sources/pickups/${kind}.webp`, import.meta.url),
  );
  if (image.width !== PICKUP_ICON_SIZE || image.height !== PICKUP_ICON_SIZE) {
    throw new Error(`Unexpected pickup icon dimensions: ${kind}`);
  }
  const segments = [
    { source: 0, size: 1, target: 0, length: PICKUP_ATLAS_PADDING },
    { source: 0, size: PICKUP_ICON_SIZE, target: PICKUP_ATLAS_PADDING, length: PICKUP_ICON_SIZE },
    {
      source: PICKUP_ICON_SIZE - 1,
      size: 1,
      target: PICKUP_ATLAS_PADDING + PICKUP_ICON_SIZE,
      length: PICKUP_ATLAS_PADDING,
    },
  ];
  for (const x of segments) {
    for (const y of segments) {
      context.drawImage(
        image,
        x.source,
        y.source,
        x.size,
        y.size,
        column * PICKUP_ATLAS_STRIDE + x.target,
        row * PICKUP_ATLAS_STRIDE + y.target,
        x.length,
        y.length,
      );
    }
  }
}
await writeFile(new URL(`../public/${PICKUP_ATLAS_PATH}`, import.meta.url), encodeWebp(canvas));
console.log(`public/${PICKUP_ATLAS_PATH}`);
