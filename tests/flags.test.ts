import assert from "node:assert/strict";
import test from "node:test";
import { spawnPositions } from "../src/game/arena";
import { Flags } from "../src/game/flags";

test("every flag owns a visible pole independent of map scenery", () => {
  const flags = new Flags();
  const poles = flags.group.children.filter((child) => child.name === "flag-pole");
  const cloth = flags.group.children.filter((child) => child.name === "flag-cloth");
  const expected = spawnPositions(0).length + spawnPositions(1).length;

  assert.equal(poles.length, expected);
  assert.equal(cloth.length, expected);
  assert.ok(poles.every((pole) => pole.position.y === 2.4));
  assert.ok(poles.every((pole) => Math.abs(pole.position.x) === 62));
});
