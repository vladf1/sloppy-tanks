import assert from "node:assert/strict";
import { test } from "node:test";
import { ordinal } from "../src/game/ui-markup";

test("the respawn card counts the killer's kills as an ordinal", () => {
  assert.deepEqual([1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111].map(ordinal), [
    "1ST",
    "2ND",
    "3RD",
    "4TH",
    "11TH",
    "12TH",
    "13TH",
    "21ST",
    "22ND",
    "101ST",
    "111TH",
  ]);
});
