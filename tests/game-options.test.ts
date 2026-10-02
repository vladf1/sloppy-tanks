import assert from "node:assert/strict";
import { test } from "node:test";
import { initialGameOptions, loadGameOptions } from "../src/game/game-options";

test("a linked map wins over the remembered map, which wins over the default", () => {
  const map = (search: string, lastMap: string | null) =>
    initialGameOptions(1, search, null, lastMap).mapMode;
  assert.equal(map("?map=harbor", "quarry"), "harbor");
  assert.equal(map("", "quarry"), "quarry");
  assert.equal(map("?map=atlantis", "quarry"), "quarry");
  assert.equal(map("", "atlantis"), "village");
  assert.equal(map("", null), "village");
});

test("extra levels are chosen from links or memory only on a page offering them", () => {
  const map = (search: string, lastMap: string | null) =>
    initialGameOptions(1, search, null, lastMap).mapMode;
  assert.equal(map("?map=superstress", "quarry"), "quarry");
  assert.equal(map("", "stress-test"), "village");
  assert.equal(map("?extralevels&map=superstress", "quarry"), "superstress");
  assert.equal(map("?extralevels", "stress-test"), "stress-test");
});

test("Battle Setup uses link/default choices when browser storage is blocked", () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    get() {
      throw new DOMException("Storage blocked", "SecurityError");
    },
  });
  try {
    assert.deepEqual(loadGameOptions(7, "?map=harbor"), initialGameOptions(7, "?map=harbor", null));
    assert.deepEqual(loadGameOptions(7, ""), initialGameOptions(7, "", null));
  } finally {
    if (original) Object.defineProperty(globalThis, "localStorage", original);
    else Reflect.deleteProperty(globalThis, "localStorage");
  }
});
