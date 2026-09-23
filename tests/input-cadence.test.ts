import { test } from "node:test";
import assert from "node:assert/strict";
import { InputCadence } from "../src/net/input-cadence";
import type { ControlInput } from "../src/net/player-controls";

const sample = (extra: Partial<ControlInput> = {}) => ({
  controlEpoch: 1,
  moveX: 0,
  moveZ: 0,
  fire: false,
  aim: { x: 10, z: 20 },
  actions: [],
  ...extra,
});

test("idle input sends once per second while held movement and fire keep their lease renewed", () => {
  for (const [input, expected] of [
    [sample(), 6],
    [sample({ moveX: 1 }), 120],
    [sample({ fire: true }), 120],
  ] as const) {
    const cadence = new InputCadence();
    let sent = 0;
    for (let now = 0; now < 6000; now += 10) {
      if (cadence.due(input, now)) {
        cadence.sent(input, now);
        sent++;
      }
    }
    assert.equal(sent, expected);
  }
});

test("aim, one-shot actions and epochs wake idle sends; movement and fire releases do not wait a second", () => {
  for (const input of [
    sample({ aim: { x: 11, z: 20 } }),
    sample({ actions: [{ type: "mine" }] }),
    sample({ actions: [{ type: "ammo", weapon: "rocket" }] }),
    sample({ controlEpoch: 2 }),
    sample({ aim: { angle: 0 } }),
  ]) {
    const cadence = new InputCadence();
    cadence.sent(sample(), 0);
    assert.equal(cadence.due(input, 49), false);
    assert.equal(cadence.due(input, 50), true);
  }
  for (const held of [sample({ moveX: 1 }), sample({ moveZ: -1 }), sample({ fire: true })]) {
    const cadence = new InputCadence();
    cadence.sent(held, 0);
    assert.equal(cadence.due(sample(), 50), true);
    cadence.sent(sample(), 50);
    assert.equal(cadence.due(sample(), 100), false);
  }
});

test("sub-centimetre camera noise does not flood idle traffic, but cumulative aim changes are sent", () => {
  const cadence = new InputCadence(),
    input = sample();
  cadence.sent(input, 0);
  input.aim = { x: 99, z: 99 };
  assert.equal(cadence.due(sample({ aim: { x: 10.005, z: 20 } }), 100), false);
  assert.equal(cadence.due(sample({ aim: { x: 10.02, z: 20 } }), 150), true);
  cadence.sent(sample({ aim: { angle: Math.PI - 0.0001 } }), 200);
  assert.equal(cadence.due(sample({ aim: { angle: -Math.PI + 0.0001 } }), 300), false);
  assert.equal(cadence.due(sample({ aim: { angle: -Math.PI + 0.002 } }), 300), true);
});
