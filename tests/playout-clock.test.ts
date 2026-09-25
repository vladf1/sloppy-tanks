import { test } from "node:test";
import assert from "node:assert/strict";
import { PlayoutClock } from "../src/net/playout-clock";

const FRAME_MS = 1000 / 60;
const TICKS_PER_BATCH = 3;
const BATCH_MS = 50;

interface Frame {
  nowMs: number;
  displayMs: number;
  newestMs: number;
  bufferMs: number;
}

/** Plays 20 Hz batches through a path whose delay per batch is `delay(batch)`, reading at 60 Hz. */
function play(seconds: number, delay: (batch: number) => number): Frame[] {
  const clock = new PlayoutClock();
  const arrivals: { atMs: number; tick: number }[] = [];
  let releaseMs = -Infinity;
  for (let batch = 1; batch * BATCH_MS <= seconds * 1000; batch++) {
    // A reliable ordered stream never delivers a batch before the one ahead of it.
    releaseMs = Math.max(releaseMs, batch * BATCH_MS + delay(batch));
    arrivals.push({ atMs: releaseMs, tick: batch * TICKS_PER_BATCH });
  }
  clock.reset(0, delay(0));
  let next = 0;
  let newestMs = 0;
  const frames: Frame[] = [];
  for (let nowMs = delay(0); nowMs <= seconds * 1000; nowMs += FRAME_MS) {
    while (next < arrivals.length && arrivals[next].atMs <= nowMs) {
      clock.arrive(arrivals[next].tick, arrivals[next].atMs);
      newestMs = (arrivals[next].tick * 1000) / 60;
      next++;
    }
    frames.push({ nowMs, displayMs: clock.read(nowMs), newestMs, bufferMs: clock.bufferMs });
  }
  return frames;
}

function assertMonotonic(frames: Frame[]): void {
  for (let i = 1; i < frames.length; i++) {
    assert.ok(frames[i].displayMs >= frames[i - 1].displayMs, "display time never runs backwards");
  }
}

test("a steady long path keeps a full buffer; RTT no longer eats the interpolation delay", () => {
  for (const pathMs of [0, 50, 100, 200]) {
    const frames = play(6, () => pathMs).filter((frame) => frame.nowMs > 1500);
    assertMonotonic(frames);
    for (const frame of frames) {
      assert.ok(
        frame.displayMs < frame.newestMs,
        `display stays behind received data at ${pathMs} ms`,
      );
    }
    for (let i = 1; i < frames.length; i++) {
      const advance = frames[i].displayMs - frames[i - 1].displayMs;
      assert.ok(advance > FRAME_MS * 0.85 && advance < FRAME_MS * 1.15, "motion never pauses");
    }
  }
});

test("ordinary arrival jitter is absorbed without underrun", () => {
  let seed = 7;
  const random = () => (seed = (seed * 1664525 + 1013904223) >>> 0) / 2 ** 32;
  const frames = play(10, () => 80 + random() * 25).filter((frame) => frame.nowMs > 3000);
  assertMonotonic(frames);
  assert.equal(frames.filter((frame) => frame.displayMs > frame.newestMs).length, 0);
  assert.ok(Math.max(...frames.map((frame) => frame.bufferMs)) < 110);
});

test("a head-of-line stall extrapolates briefly, grows the buffer, then gives the delay back", () => {
  const stalled = 60; // Batch 60 (3 s) is retransmitted 200 ms late; later batches queue behind it.
  const frames = play(20, (batch) => (batch === stalled ? 230 : 30));
  assertMonotonic(frames);
  for (const frame of frames) {
    assert.ok(frame.displayMs <= frame.newestMs + 100, "extrapolation is bounded");
  }
  const starved = frames.filter((frame) => frame.displayMs > frame.newestMs);
  assert.ok(starved.length > 0 && starved.length * FRAME_MS < 200, "only part of the stall shows");
  const peak = Math.max(...frames.map((frame) => frame.bufferMs));
  assert.ok(peak > 100, "the stall grows the buffer");
  assert.ok(frames.at(-1)!.bufferMs < 75, "the buffer shrinks once arrivals are steady again");
});

test("repeated stalls buy enough buffer to hide the next one", () => {
  // Every second, one batch is retransmitted 150 ms late on a 40 ms path.
  const frames = play(12, (batch) => (batch % 20 === 0 ? 190 : 40));
  assertMonotonic(frames);
  const settled = frames.filter((frame) => frame.nowMs > 4000);
  const starved = settled.filter((frame) => frame.displayMs > frame.newestMs);
  assert.equal(starved.length, 0, "motion no longer pauses once the stall pattern is learned");
  assert.ok(Math.max(...settled.map((frame) => frame.bufferMs)) <= 250);
});

test("reset starts behind the baseline and a long gap snaps forward", () => {
  const clock = new PlayoutClock();
  clock.reset(600, 1000);
  assert.ok(clock.read(1000) < 10_000);
  clock.arrive(1200, 11_000);
  const displayMs = clock.read(11_000);
  assert.ok(displayMs > 19_800 && displayMs <= 20_000, "a long gap is not slewed through");
});
