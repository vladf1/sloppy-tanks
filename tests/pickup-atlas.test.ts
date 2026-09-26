import assert from "node:assert/strict";
import { test } from "node:test";
import { createCanvas, loadImage } from "@napi-rs/canvas";
import { PICKUPS } from "../src/game/data";
import {
  PICKUP_ATLAS_PATH,
  PICKUP_ATLAS_PADDING,
  PICKUP_ATLAS_SIZE,
  PICKUP_ATLAS_STRIDE,
  PICKUP_ATLAS_TILES,
  PICKUP_ICON_SIZE,
  pickupAtlasUV,
} from "../src/game/pickup-atlas";
import type { PickupKind } from "../src/game/types";

test("pickup atlas preserves all icon pixels, UV orientation and extruded gutters", async () => {
  assert.deepEqual(Object.keys(PICKUP_ATLAS_TILES).sort(), Object.keys(PICKUPS).sort());
  const atlas = await loadImage(new URL(`../public/${PICKUP_ATLAS_PATH}`, import.meta.url));
  assert.equal(atlas.width, PICKUP_ATLAS_SIZE);
  assert.equal(atlas.height, PICKUP_ATLAS_SIZE);
  const packed = createCanvas(atlas.width, atlas.height).getContext("2d");
  packed.drawImage(atlas, 0, 0);
  for (const kind of Object.keys(PICKUP_ATLAS_TILES) as PickupKind[]) {
    const source = await loadImage(
      new URL(`../assets/texture-sources/pickups/${kind}.webp`, import.meta.url),
    );
    const original = createCanvas(source.width, source.height).getContext("2d");
    original.drawImage(source, 0, 0);
    const [column, row] = PICKUP_ATLAS_TILES[kind];
    const x = column * PICKUP_ATLAS_STRIDE + PICKUP_ATLAS_PADDING;
    const y = row * PICKUP_ATLAS_STRIDE + PICKUP_ATLAS_PADDING;
    assert.deepEqual(
      packed.getImageData(x, y, PICKUP_ICON_SIZE, PICKUP_ICON_SIZE).data,
      original.getImageData(0, 0, source.width, source.height).data,
      `${kind}: source artwork must remain lossless`,
    );
    for (const dx of [-PICKUP_ATLAS_PADDING, -1, 128, 256, 271]) {
      for (const dy of [-PICKUP_ATLAS_PADDING, -1, 128, 256, 271]) {
        assert.deepEqual(
          packed.getImageData(x + dx, y + dy, 1, 1).data,
          original.getImageData(
            Math.max(0, Math.min(255, dx)),
            Math.max(0, Math.min(255, dy)),
            1,
            1,
          ).data,
          `${kind}: gutter pixel ${dx},${dy}`,
        );
      }
    }
    for (const [u, v] of [
      [0, 0],
      [0, 1],
      [1, 0],
      [1, 1],
    ]) {
      const [atlasU, atlasV] = pickupAtlasUV(kind, u, v);
      assert.ok(Math.abs(atlasU * atlas.width - (x + u * source.width)) < 1e-9);
      assert.ok(Math.abs((1 - atlasV) * atlas.height - (y + (1 - v) * source.height)) < 1e-9);
    }
  }
});
