import { test } from "node:test";
import assert from "node:assert/strict";
import { RateLimit } from "../server/rate-limit";

test("rate limit allows a fixed number of calls per key and window", () => {
  const limit = new RateLimit(2, 1000);
  assert.equal(limit.allow("a", 0), true);
  assert.equal(limit.allow("a", 10), true);
  assert.equal(limit.allow("a", 20), false);
  assert.equal(limit.allow("b", 20), true);
  assert.equal(limit.allow("a", 1000), true);
});

test("rate limit refuses new keys while every tracked window is live, then frees expired ones", () => {
  const limit = new RateLimit(5, 1000, 3);
  for (const key of ["a", "b", "c"]) assert.equal(limit.allow(key, 0), true);
  // A flood of fresh addresses cannot grow the map past its cap.
  for (let index = 0; index < 100; index++) assert.equal(limit.allow("new" + index, 500), false);
  assert.equal(limit.allow("a", 500), true, "tracked keys keep their budget");
  assert.equal(limit.allow("d", 1000), true, "expired windows make room");
  assert.equal(limit.allow("e", 1000), true);
  assert.equal(limit.allow("f", 1000), true);
  assert.equal(limit.allow("g", 1000), false);
});
