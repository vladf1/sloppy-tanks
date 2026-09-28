import { test } from "node:test";
import assert from "node:assert/strict";
import { FirstPersonLook, viewRelativeMove } from "../src/game/first-person";
import { idleCommand } from "../src/game/types";
import { FIRST_PERSON } from "../src/game/view-settings";

const close = (actual: number, expected: number) =>
  assert.ok(Math.abs(actual - expected) < 1e-9, `${actual} ≈ ${expected}`);

test("view-relative movement is the overhead mapping when facing the overhead camera's way", () => {
  for (const [moveX, moveZ] of [
    [1, 0],
    [0, -1],
    [-0.5, 0.5],
  ]) {
    const moved = viewRelativeMove(moveX, moveZ, Math.PI);
    close(moved.moveX, moveX);
    close(moved.moveZ, moveZ);
  }
});

test("forward drives along the view and right drives to its right", () => {
  for (const yaw of [0, 0.7, -2.1, Math.PI / 2]) {
    const forward = viewRelativeMove(0, -1, yaw);
    close(forward.moveX, Math.sin(yaw));
    close(forward.moveZ, Math.cos(yaw));
    // Turning right lowers yaw, so the right side is a quarter turn below the view.
    const right = viewRelativeMove(1, 0, yaw);
    close(right.moveX, Math.sin(yaw - Math.PI / 2));
    close(right.moveZ, Math.cos(yaw - Math.PI / 2));
  }
});

test("a rotated keyboard diagonal stays within the multiplayer input range", () => {
  for (const yaw of [0, Math.PI / 4, 0.7, -2.1, Math.PI / 2]) {
    for (const [moveX, moveZ] of [
      [1, -1],
      [-1, -1],
      [1, 1],
    ]) {
      const moved = viewRelativeMove(moveX, moveZ, yaw);
      close(Math.hypot(moved.moveX, moved.moveZ), 1);
      assert.ok(Math.abs(moved.moveX) <= 1 && Math.abs(moved.moveZ) <= 1);
      const direction = viewRelativeMove(moveX / Math.SQRT2, moveZ / Math.SQRT2, yaw);
      close(moved.moveX, direction.moveX);
      close(moved.moveZ, direction.moveZ);
    }
  }
});

test("entering first person starts from the turret aim and steering follows the view", () => {
  const look = new FirstPersonLook();
  const command = { ...idleCommand(), moveZ: -1, aim: 0.3, fire: true };
  assert.equal(look.steer(command), command);
  look.toggle(0.3);
  assert.equal(look.enabled, true);
  close(look.yaw, 0.3);
  const steered = look.steer(command);
  close(steered.moveX, Math.sin(0.3));
  close(steered.moveZ, Math.cos(0.3));
  assert.deepEqual([steered.aim, steered.fire], [0.3, true]);
  look.toggle(-1);
  assert.equal(look.enabled, false);
  close(look.yaw, 0.3);
});

test("mouse and aim stick turn right by lowering yaw, wrapped to one turn", () => {
  const look = new FirstPersonLook();
  look.toggle(0);
  look.turn(100, 0, 0);
  close(look.yaw, -100 * FIRST_PERSON.mouseRadiansPerPixel);
  look.turn(0, -1, 0.5);
  close(
    look.yaw,
    -100 * FIRST_PERSON.mouseRadiansPerPixel + 0.5 * FIRST_PERSON.touchTurnRadiansPerSecond,
  );
  look.toggle(0);
  look.toggle(Math.PI - 0.01);
  look.turn(-0.02 / FIRST_PERSON.mouseRadiansPerPixel, 0, 0);
  close(look.yaw, -Math.PI + 0.01);
});

test("screen angles put the view straight up and clockwise to the right", () => {
  const look = new FirstPersonLook();
  look.toggle(1);
  close(look.screenAngle(1), 0);
  close(look.screenAngle(1 - Math.PI / 2), Math.PI / 2);
  close(look.screenAngle(1 + Math.PI / 2), -Math.PI / 2);
});
