import { test } from "node:test";
import assert from "node:assert/strict";
import { DelayedChannel } from "../src/net/delayed-channel";
test("transport jitter preserves reliable message order and due time", () => {
  const channel = new DelayedChannel<string>();
  channel.send("old", 0, 100);
  channel.send("new", 20, 10);
  assert.deepEqual(channel.receive(30), []);
  assert.deepEqual(channel.receive(99), []);
  assert.deepEqual(channel.receive(100), ["old", "new"]);
  assert.equal(channel.size, 0);
});
test("delay buffers are bounded and reset discards old-life actions", () => {
  const channel = new DelayedChannel<string>(2);
  channel.send("mine", 0, 50);
  channel.send("fire", 0, 50);
  assert.throws(() => channel.send("overflow", 0, 1), /capacity/);
  channel.clear();
  assert.deepEqual(channel.receive(1000), []);
  assert.throws(() => channel.send("invalid", 0, NaN), /Invalid/);
});
