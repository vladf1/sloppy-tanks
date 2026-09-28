import { mock, test } from "node:test";
import assert from "node:assert/strict";
import { afterPaint, nextTask, withTaskYield } from "../src/game/task-yield";

type Host = { scheduler?: { yield?: () => Promise<void> } };
const host = globalThis as Host;

test("shader warm-up yields by task where scheduler.yield is missing, then cleans up", async () => {
  const original = host.scheduler;
  delete host.scheduler;
  try {
    let yields = 0;
    // Node has no requestAnimationFrame: these would never settle via Three's fallback.
    await withTaskYield(async () => {
      for (let i = 0; i < 50; i++) {
        await host.scheduler!.yield!();
        yields++;
      }
      await withTaskYield(async () => {
        await host.scheduler!.yield!();
      });
      assert.ok(host.scheduler?.yield, "nested warm-ups keep the shim");
    });
    assert.equal(yields, 50);
    assert.equal(host.scheduler, undefined, "no partial Scheduler remains afterwards");
    const native = { yield: async () => {} };
    host.scheduler = native;
    await withTaskYield(async () => {
      assert.equal(host.scheduler, native);
      assert.equal(host.scheduler.yield, native.yield, "a native yield is left alone");
    });
  } finally {
    if (original) host.scheduler = original;
    else delete host.scheduler;
  }
});

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
    await paint;

    // A hidden tab never runs the frame callback.
    let waited = false;
    const hidden = afterPaint().then(() => (waited = true));
    await nextTask();
    assert.equal(waited, false);
    mock.timers.tick(100);
    await hidden;
  } finally {
    mock.timers.reset();
    if (original) host.requestAnimationFrame = original;
    else delete host.requestAnimationFrame;
  }
});
