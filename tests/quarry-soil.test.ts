import { test } from "node:test";
import assert from "node:assert/strict";
import { ACCUM_CELLS, bakeQuarrySoil, QUARRY_SOIL_SIZE } from "../src/game/quarry-soil";

test("quarry soil bands bake the same pixels as one continuous pass", () => {
  const accum = Float32Array.from({ length: ACCUM_CELLS * ACCUM_CELLS }, (_, i) => (i % 7) / 10);
  const row = QUARRY_SOIL_SIZE * 4;
  const whole = bakeQuarrySoil(accum, 0, 24);
  const split = [...bakeQuarrySoil(accum, 0, 9), ...bakeQuarrySoil(accum, 9, 24)];
  assert.deepEqual(split, [...whole]);
  // A worker band deep in the image replays the shared stream's earlier draws.
  const late = bakeQuarrySoil(accum, 1200, 1203);
  assert.deepEqual(late, bakeQuarrySoil(accum, 1197, 1203).slice(3 * row));
});
