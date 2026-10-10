import { test } from "node:test";
import assert from "node:assert/strict";
import { TouchInput } from "../src/game/touch-input";

test("each control has one finger and analog movement is bounded", () => {
  const input = new TouchInput();
  assert.equal(input.begin("drive", 1), true);
  assert.equal(input.begin("arena", 1), false, "one finger owns one control");
  assert.equal(input.begin("drive", 3), false);
  assert.equal(input.begin("arena", 2), true);
  input.moveStick(3, 1, 1);
  assert.equal(input.moveX, 0);
  input.moveStick(1, 0.05, 0.05);
  assert.equal(input.moveX, 0);
  input.moveStick(1, 0.56, 0);
  assert.ok(Math.abs(input.moveX - 0.5) < 1e-8);
  input.moveStick(1, 3, 4);
  assert.equal(Math.hypot(input.moveX, input.moveZ), 1);
  input.end("drive", 1);
  assert.equal(input.moveX, 0);
  assert.equal(input.moveZ, 0);
  assert.equal(input.fire, true, "releasing the stick keeps the arena finger firing");
});

test("an arena finger fires until it lifts", () => {
  const input = new TouchInput();
  assert.equal(input.begin("arena", 1), true);
  assert.equal(input.fire, true);
  assert.equal(input.begin("arena", 2), false, "one arena finger owns the shot");
  input.end("arena", 2);
  assert.equal(input.fire, true, "another finger cannot release it");
  input.end("arena", 1);
  assert.equal(input.fire, false);
});

test("clear drops captured input and stale moves cannot resume it", () => {
  const input = new TouchInput();
  input.begin("drive", 1);
  input.begin("arena", 2);
  input.moveStick(1, 1, 0);
  input.clear();
  input.moveStick(1, 1, 0);
  assert.equal(input.moveX, 0);
  assert.equal(input.fire, false);
  assert.deepEqual(input.pointers, { drive: null, arena: null });
  assert.equal(input.begin("drive", 1), true);
});

test("clearing idle desktop input does not notify the touch UI", () => {
  const input = new TouchInput();
  let notifications = 0;
  input.changed = () => notifications++;
  for (let i = 0; i < 120; i++) input.clear();
  assert.equal(notifications, 0);
  input.begin("drive", 1);
  input.clear();
  assert.equal(notifications, 1);
  input.clear();
  assert.equal(notifications, 1);
});
