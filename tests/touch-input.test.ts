import { test } from "node:test";
import assert from "node:assert/strict";
import { TouchInput } from "../src/game/touch-input";

test("two sticks have independent pointer ownership and analog movement is bounded", () => {
  const input = new TouchInput();
  assert.equal(input.begin("drive", 1), true);
  assert.equal(input.begin("aim", 1), false);
  assert.equal(input.begin("drive", 3), false);
  assert.equal(input.begin("aim", 2), true);
  input.move("drive", 3, 1, 1);
  assert.equal(input.moveX, 0);
  input.move("drive", 1, 0.05, 0.05);
  assert.equal(input.moveX, 0);
  input.move("drive", 1, 0.56, 0);
  assert.ok(Math.abs(input.moveX - 0.5) < 1e-8);
  input.move("drive", 1, 3, 4);
  assert.equal(Math.hypot(input.moveX, input.moveZ), 1);
  input.end("drive", 1);
  assert.equal(input.moveX, 0);
  assert.equal(input.moveZ, 0);
  input.move("aim", 2, 0, -1);
  input.end("aim", 2);
  assert.equal(input.aiming, true);
  assert.equal(input.aimY, -1);
});

test("the aim stick only aims; the fire button fires while its finger is down", () => {
  const input = new TouchInput();
  input.begin("aim", 9);
  input.move("aim", 9, 1, 0);
  assert.equal(input.aimX, 1);
  assert.equal(input.fire, false, "a full stick push no longer fires");
  input.move("aim", 9, 0.05, 0);
  assert.equal(input.aimX, 1, "the dead zone keeps the last aim");
  assert.equal(input.begin("fire", 9), false, "one finger owns one control");
  assert.equal(input.begin("fire", 4), true);
  assert.equal(input.fire, true);
  assert.equal(input.begin("fire", 5), false);
  input.end("aim", 9);
  assert.equal(input.fire, true, "releasing the aim stick keeps firing");
  input.end("fire", 5);
  assert.equal(input.fire, true, "another finger cannot release the button");
  input.end("fire", 4);
  assert.equal(input.fire, false);
  assert.equal(input.aimX, 1);
});

test("an arena finger fires until it lifts, alongside the fire button", () => {
  const input = new TouchInput();
  assert.equal(input.begin("arena", 1), true);
  assert.equal(input.fire, true);
  assert.equal(input.begin("arena", 2), false, "one arena finger owns the shot");
  assert.equal(input.begin("fire", 3), true);
  input.end("arena", 1);
  assert.equal(input.fire, true, "the fire button still holds fire");
  input.end("fire", 3);
  assert.equal(input.fire, false);
});

test("clear drops captured input and stale moves cannot resume it", () => {
  const input = new TouchInput();
  input.begin("drive", 1);
  input.begin("aim", 2);
  input.begin("fire", 3);
  input.move("drive", 1, 1, 0);
  input.move("aim", 2, 1, 0);
  input.clear();
  input.move("drive", 1, 1, 0);
  input.move("aim", 2, 1, 0);
  assert.equal(input.moveX, 0);
  assert.equal(input.fire, false);
  assert.equal(input.aiming, false);
  assert.deepEqual(input.pointers, { drive: null, aim: null, fire: null, arena: null });
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
