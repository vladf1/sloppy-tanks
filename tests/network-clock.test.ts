import { test } from "node:test";
import assert from "node:assert/strict";
import { FixedStepClock } from "../src/net/fixed-step-clock";
import { DelayedChannel } from "../src/net/transport-delay";

test("50 ms host batches preserve exactly sixty simulation ticks per second", () => {
  const clock = new FixedStepClock(0);
  const ticks: number[] = [];
  for (let now = 50; now <= 10000; now += 50) {
    assert.equal(
      clock.advance(now, (tick) => ticks.push(tick)),
      true,
    );
  }
  assert.equal(ticks.length, 600);
  assert.equal(clock.tick, 600);
  assert.ok(clock.debtMs < 1e-6);
});
test("catch-up is bounded and retains debt instead of skipping physics", () => {
  const clock = new FixedStepClock(1000);
  assert.equal(
    clock.advance(1200, () => {}),
    true,
  );
  assert.equal(clock.tick, 6);
  assert.ok(Math.abs(clock.debtMs - 100) < 1e-6);
  clock.advance(1250, () => {});
  assert.equal(clock.tick, 12);
  assert.ok(Math.abs(clock.debtMs - 50) < 1e-6);
  clock.advance(1300, () => {});
  assert.equal(clock.tick, 18);
  assert.ok(clock.debtMs < 1e-6);
});
test("overload fails before executing an unbounded batch; backwards clocks add no time", () => {
  const clock = new FixedStepClock(100);
  clock.advance(90, () => assert.fail("No tick is due"));
  clock.advance(150, () => {});
  assert.equal(clock.tick, 3);
  assert.equal(
    clock.advance(500, () => assert.fail("Overload must terminate")),
    false,
  );
  assert.equal(clock.tick, 3);
  assert.throws(() => clock.advance(NaN, () => {}), /Invalid host clock/);
});
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
