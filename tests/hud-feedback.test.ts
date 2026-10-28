// The engine words the death report, the kill feed, toasts, effects and the hull's tone
// (`crates/web/src/feedback.rs` and `hud.rs`, tested there); the page lays them out.
import assert from "node:assert/strict";
import { test } from "node:test";
import { cssColor, damageEdgePoint, damageRingPlacement } from "../src/game/hud-feedback";
import { teamOutcome } from "../src/game/ui-markup";

test("the damage ring wraps the tank overhead and opens to the screen edge in first person", () => {
  assert.deepEqual(damageRingPlacement({ x: 0.5, y: -0.5 }), {
    left: "75.00%",
    top: "75.00%",
    edge: false,
  });
  // Off-screen positions stay on the screen.
  assert.equal(damageRingPlacement({ x: 3, y: 0 }).left, "100.00%");
  assert.deepEqual(damageRingPlacement(null), { left: "50%", top: "50%", edge: true });
  // Straight up is the top edge's middle; a quarter turn clockwise is the right edge.
  assert.deepEqual(damageEdgePoint(0), { x: "50.0%", y: "0.0%" });
  assert.deepEqual(damageEdgePoint(Math.PI / 2), { x: "100.0%", y: "50.0%" });
  assert.deepEqual(damageEdgePoint(Math.PI / 4), { x: "100.0%", y: "0.0%" });
});

test("the engine's hull ramp colour reads as a six-digit CSS colour", () => {
  assert.equal(cssColor(0x5fe39a), "#5fe39a");
  assert.equal(cssColor(0x00a0ff), "#00a0ff");
});

test("the battle report names a win, a loss or a draw, even for a battle ended early", () => {
  assert.equal(teamOutcome([100, 80], 0, 0, false), "won");
  assert.equal(teamOutcome([100, 80], 1, 0, false), "lost");
  // Ended early: the side ahead when it ended takes it; level scores are a draw.
  assert.equal(teamOutcome([3, 4], 1, null, true), "won");
  assert.equal(teamOutcome([3, 4], 0, null, true), "lost");
  assert.equal(teamOutcome([0, 0], 0, null, true), "draw");
});
