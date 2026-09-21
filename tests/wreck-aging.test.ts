import { test } from "node:test";
import assert from "node:assert/strict";
import * as THREE from "three";
import { ageWreckMaterial } from "../src/game/wreck-aging";

test("wreck paint starts dimmed and fades to 20 percent brightness without compounding", () => {
  const live = new THREE.MeshStandardMaterial({ color: 0xdb3838, emissive: 0x331010 });
  const wreck = live.clone();
  const color = live.color.clone();
  const emissive = live.emissive.clone();
  for (const [age, multiplier] of [
    [0, 0.8],
    [1.25, 0.5],
    [2.5, 0.2],
    [20, 0.2],
    [20, 0.2],
  ]) {
    ageWreckMaterial(wreck, age);
    for (const channel of ["r", "g", "b"] as const) {
      assert.ok(Math.abs(wreck.color[channel] - color[channel] * multiplier) < 1e-7);
      assert.ok(Math.abs(wreck.emissive[channel] - emissive[channel] * multiplier) < 1e-7);
    }
    assert.ok(live.color.equals(color));
    assert.ok(live.emissive.equals(emissive));
  }
});
