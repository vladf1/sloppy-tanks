import { test } from "node:test";
import assert from "node:assert/strict";
import { TankSuspension } from "../src/game/tank-suspension";

test("acceleration lifts the nose, braking dips it, and a stopped hull settles flat", () => {
  for (const heading of [0, Math.PI / 2, Math.PI, -Math.PI / 2]) {
    const suspension = new TankSuspension();
    let time = 0;
    const step = (speed: number) => {
      suspension.update(
        Math.sin(heading) * speed,
        Math.cos(heading) * speed,
        heading,
        time,
        1 / 60,
      );
      time += 1 / 60;
    };
    step(0);
    for (let i = 1; i <= 12; i++) step(i / 2);
    assert.ok(suspension.pitch.angle < -0.005);
    assert.ok(Math.abs(suspension.roll.angle) < 1e-8);
    for (let i = 0; i < 30; i++) step(6);
    for (let i = 11; i >= 0; i--) step(i / 2);
    assert.ok(suspension.pitch.angle > 0.005);
    for (let i = 0; i < 120; i++) step(0);
    assert.ok(Math.abs(suspension.pitch.angle) < 1e-8);
  }
});

test("lateral acceleration leans outward but stationary pivoting does not rock the hull", () => {
  const right = new TankSuspension();
  const left = new TankSuspension();
  const pivot = new TankSuspension();
  for (let i = 0; i < 15; i++) {
    right.update(i / 3, 4, 0, i / 60, 1 / 60);
    left.update(-i / 3, 4, 0, i / 60, 1 / 60);
    pivot.update(0, 0, i / 5, i / 60, 1 / 60);
  }
  assert.ok(right.roll.angle > 0.005);
  assert.equal(right.roll.angle, -left.roll.angle);
  assert.equal(pivot.pitch.angle, 0);
  assert.equal(pivot.roll.angle, 0);
});

test("suspension stays bounded through impacts, pauses and long render gaps", () => {
  const suspension = new TankSuspension();
  suspension.update(0, 0, 0, 0, 1 / 60);
  for (let i = 1; i <= 120; i++) {
    suspension.update(i % 2 ? 1000 : -1000, i % 3 ? 1000 : -1000, i / 10, i / 60, 1 / 60);
    assert.ok(Math.abs(suspension.pitch.angle) <= 0.035);
    assert.ok(Math.abs(suspension.roll.angle) <= 0.025);
  }
  const pitch = suspension.pitch.angle;
  const roll = suspension.roll.angle;
  suspension.update(0, 0, 0, 2, 0);
  assert.equal(suspension.pitch.angle, pitch);
  assert.equal(suspension.roll.angle, roll);
  suspension.update(0, 0, 0, 30, 10);
  assert.ok(Number.isFinite(suspension.pitch.angle));
  for (let i = 1; i <= 120; i++) suspension.update(0, 0, 0, 30 + i / 60, 1 / 60);
  assert.ok(Math.abs(suspension.pitch.angle) < 1e-8);
  assert.ok(Math.abs(suspension.roll.angle) < 1e-8);
});

test("30, 60 and 120 FPS produce similar motion with a 60 Hz physics clock", () => {
  const sample = (fps: number) => {
    const suspension = new TankSuspension();
    let peak = 0;
    for (let i = 0; i <= fps; i++) {
      const time = Math.floor((i / fps) * 60 + 1e-8) / 60;
      const speed = Math.min(time * 30, 6);
      suspension.update(0, speed, 0, time, 1 / fps);
      peak = Math.max(peak, -suspension.pitch.angle);
    }
    return peak;
  };
  const peaks = [30, 60, 120].map(sample);
  assert.ok(Math.max(...peaks) - Math.min(...peaks) < 0.003, String(peaks));
});
