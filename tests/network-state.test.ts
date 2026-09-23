import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { idleCommand } from "../src/game/types";
import {
  ENTITY_TYPES,
  experimentEvent,
  experimentState,
  ExperimentStream,
} from "../src/net/experiment-state";

before(async () => {
  await RAPIER.init();
});
function assertWire(value: unknown): void {
  if (typeof value === "number") assert.ok(Number.isFinite(value));
  if (value && typeof value === "object") {
    assert.ok(Array.isArray(value) || Object.getPrototypeOf(value) === Object.prototype);
    for (const [key, item] of Object.entries(value)) {
      assert.ok(!["body", "collider", "handle", "world"].includes(key), key);
      assertWire(item);
    }
  }
}
for (const mapMode of ["village", "harbor", "quarry"] as const) {
  test(`M1 ${mapMode}: JSON deltas reconstruct full state through a collapse and removals`, () => {
    const simulation = new Simulation(4242, { mapMode });
    const stream = new ExperimentStream();
    const mirror = new Map<string, string>();
    let eventId = 0;
    try {
      simulation.start();
      for (let tick = 1; tick <= 120; tick++) {
        if (tick === 31) {
          for (const cover of simulation.covers
            .filter((c) => c.alive && c.destructible)
            .slice(0, 8)) {
            simulation.damageCover(cover, 10000, -1, 0);
          }
        }
        simulation.step(idleCommand(), true);
        const events = simulation.events
          .splice(0)
          .map((event) => experimentEvent(event, tick, ++eventId));
        assertWire(events);
        if (tick % 3) continue;
        const state = experimentState(simulation);
        assertWire(state);
        const snapshot = stream.snapshot(state, tick, events);
        const decoded = JSON.parse(JSON.stringify(snapshot)) as typeof snapshot;
        for (const { kind, entity } of decoded.updates)
          mirror.set(`${kind}:${entity.id}`, JSON.stringify(entity));
        for (const key of decoded.removed) mirror.delete(key);
        const expected = new Map<string, string>();
        for (const kind of ENTITY_TYPES) {
          for (const entity of state[kind])
            expected.set(`${kind}:${entity.id}`, JSON.stringify(entity));
        }
        assert.deepEqual(mirror, expected);
        const full = stream.full(state, tick, eventId);
        assert.equal(full.seq, snapshot.seq, "joining must not advance the shared stream");
        assert.deepEqual(JSON.parse(JSON.stringify(full)).state, JSON.parse(JSON.stringify(state)));
      }
      assert.ok(
        experimentState(simulation).covers.some(
          (cover) => cover.indestructible && cover.hp === null,
        ),
      );
    } finally {
      simulation.dispose();
    }
  });
}
test("event projection excludes accidental physics fields in explosion sources", () => {
  const input = { type: "explosion" as const, x: 1, z: 2, body: { handle: 42 } };
  const event = experimentEvent(input, 3, 4);
  assertWire(event);
  assert.equal("body" in event, false);
  assert.equal(event.tick, 3);
});
