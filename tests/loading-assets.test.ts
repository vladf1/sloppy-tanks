import assert from "node:assert/strict";
import { test } from "node:test";
import { LoadingManager } from "three/webgpu";
import { trackAssetLoading } from "../src/game/loading-assets";

test("asset barrier waits for overlapping loads and permits failed-image fallback", async () => {
  const manager = new LoadingManager();
  const wait = trackAssetLoading(manager);
  await wait();
  manager.itemStart("ground");
  manager.itemStart("trees");
  let completed = false;
  const ready = wait().then(() => {
    completed = true;
  });
  manager.itemEnd("ground");
  await Promise.resolve();
  assert.equal(completed, false);
  manager.itemError("trees");
  manager.itemEnd("trees");
  await ready;
  assert.equal(completed, true);
  manager.itemStart("next-map");
  const next = wait();
  manager.itemEnd("next-map");
  await next;
});
