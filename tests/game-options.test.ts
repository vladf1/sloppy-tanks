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
  assert.equal(map("?debug&map=superstress", "quarry"), "superstress");
  assert.equal(map("?debug", "stress-test"), "stress-test");
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
  assert.equal(options("?debug").gameMode, "team");
  assert.equal(options("?debug&map=harbor").gameMode, "solo");
  assert.equal(options("").gameMode, "solo");
});

test("Battle Setup loads all saved choices through the shared preference reader", () => {
  const original = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  const stored = new Map([
    ["sloppy-difficulty", "hard"],
    ["sloppy-map", "harbor"],
    ["sloppy-tank", "heavy"],
    ["sloppy-game-mode", "solo"],
  ]);
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: { getItem: (key: string) => stored.get(key) ?? null },
  });
  try {
    assert.deepEqual(
      loadGameOptions(7, ""),
      initialGameOptions(7, "", "hard", "harbor", "heavy", "solo"),
    );
    assert.equal(loadGameOptions(7, "?map=quarry").mapMode, "quarry");
  } finally {
    if (original) Object.defineProperty(globalThis, "localStorage", original);
    else Reflect.deleteProperty(globalThis, "localStorage");
  }
});
