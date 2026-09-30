import { mock, test } from "node:test";
import assert from "node:assert/strict";
import { afterPaint, nextTask } from "../src/game/task-yield";

test("nextTask resolves in a later task, after pending promise work", async () => {
  const order: string[] = [];
  const next = nextTask().then(() => order.push("task"));
  await Promise.resolve();
  order.push("microtask");
  await next;
  assert.deepEqual(order, ["microtask", "task"]);
});

test("afterPaint waits for the next frame and a task after it, or a timer when none comes", async () => {
  const frames: (() => void)[] = [];
  const host = globalThis as { requestAnimationFrame?: (callback: () => void) => number };
  const original = host.requestAnimationFrame;
  host.requestAnimationFrame = (callback) => frames.push(callback);
  mock.timers.enable({ apis: ["setTimeout"] });
  try {
    let painted = false;
    const paint = afterPaint().then(() => (painted = true));
    await nextTask();
    assert.equal(painted, false, "nothing runs before the frame");
    frames.shift()!();
    await Promise.resolve();
    assert.equal(painted, false, "the frame callback's microtasks run before painting");
    await paint;
    assert.equal(painted, true);

    // A hidden tab never runs the frame callback.
    let waited = false;
    const hidden = afterPaint().then(() => (waited = true));
    await nextTask();
    assert.equal(waited, false);
    mock.timers.tick(99);
    await Promise.resolve();
    assert.equal(waited, false, "the fallback waits for its full timeout");
    mock.timers.tick(1);
    await hidden;
    assert.equal(waited, true);
  } finally {
    mock.timers.reset();
    if (original) host.requestAnimationFrame = original;
    else delete host.requestAnimationFrame;
  }
});
