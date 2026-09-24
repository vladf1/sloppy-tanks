import assert from "node:assert/strict";
import { test } from "node:test";
import { initialGameOptions } from "../src/game/game-options";

test("a linked map wins over the remembered map, which wins over the default", () => {
  const map = (search: string, lastMap: string | null) =>
    initialGameOptions(1, search, null, lastMap).mapMode;
  assert.equal(map("?map=harbor", "quarry"), "harbor");
  assert.equal(map("", "quarry"), "quarry");
  assert.equal(map("?map=atlantis", "quarry"), "quarry");
  assert.equal(map("", "atlantis"), "village");
  assert.equal(map("", null), "village");
});
