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

test("extra levels are chosen from links or memory only on a page offering them", () => {
  const map = (search: string, lastMap: string | null) =>
    initialGameOptions(1, search, null, lastMap).mapMode;
  assert.equal(map("?map=superstress", "quarry"), "quarry");
  assert.equal(map("", "stress-test"), "village");
  assert.equal(map("?extralevels&map=superstress", "quarry"), "superstress");
  assert.equal(map("?extralevels", "stress-test"), "stress-test");
});

test("returning players keep their tank, standard battle format, map and difficulty", () => {
  const options = initialGameOptions(1, "", "hard", "harbor", "heavy", "solo");
  assert.equal(options.humanKind, "heavy");
  assert.equal(options.gameMode, "solo");
  assert.equal(options.mapMode, "harbor");
  assert.equal(options.difficulty, "hard");
  const defaults = initialGameOptions(1, "", "invalid", null, "humvee", "invalid");
  assert.equal(defaults.humanKind, "balanced");
  assert.equal(defaults.gameMode, "team");
  assert.equal(defaults.difficulty, "normal");
});

test("extra levels force team battle while a stored Solo Assault returns on standard maps", () => {
  const options = (search: string) =>
    initialGameOptions(1, search, "easy", "superstress", "scout", "solo");
  assert.equal(options("?extralevels").gameMode, "team");
  assert.equal(options("?extralevels&map=harbor").gameMode, "solo");
  assert.equal(options("").gameMode, "solo");
});
