import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { createMultiplayerSimulation } from "../src/net/multiplayer-simulation";
import { PlayerControls, encodeInput, type ControlInput } from "../src/net/player-controls";
before(async () => {
  await RAPIER.init();
});
function setup() {
  const sim = createMultiplayerSimulation(4242, [
    { playerId: "alice", name: "Alice", team: 0, slot: 0, kind: "balanced" },
  ]);
  const tank = sim.human;
  const controls = new PlayerControls(tank, 0);
  const input = (seq: number, extra: Partial<ControlInput> = {}): ControlInput => ({
    controlEpoch: controls.controlEpoch,
    seq,
    observedTick: 0,
    moveX: 1,
    moveZ: 0,
    aim: { x: 30, z: 40 },
    fire: true,
    actions: [],
    ...extra,
  });
  return { sim, tank, controls, input };
}
test("input holds until its lease expires, then idles and eventually hands control to a bot", () => {
  const { sim, tank, controls, input } = setup();
  try {
    assert.equal(controls.accept(input(1), 0, 0), true);
    assert.equal(controls.command(1, 0)?.moveX, 1);
    assert.equal(controls.command(14, 249)?.fire, true);
    assert.equal(controls.command(15, 250)?.fire, false);
    assert.equal(controls.command(15, 250)?.moveX, 0);
    assert.equal(controls.command(299, 4999)?.fire, false);
    const epoch = controls.controlEpoch;
    assert.equal(controls.command(300, 5000), undefined);
    assert.equal(tank.driver, "bot");
    assert.equal(tank.human, true);
    assert.ok(controls.controlEpoch > epoch);
    assert.equal(
      controls.accept(input(2), 300, 5010),
      false,
      "late input cannot silently reclaim a bot-driven seat",
    );
    controls.resume(5010);
    assert.equal(tank.driver, "human");
    assert.equal(controls.command(301, 5010)?.fire, false);
    assert.equal(controls.accept(input(1, { observedTick: 301 }), 301, 5011), true);
    assert.equal(controls.command(302, 5012)?.fire, true);
  } finally {
    sim.dispose();
  }
});
test("ordered actions survive coalesced inputs, run once, and stale clicks expire independently", () => {
  const { sim, controls, input } = setup();
  try {
    assert.equal(
      controls.accept(
        input(1, { actions: [{ type: "mine" }, { type: "ammo", weapon: "rocket" }] }),
        0,
        0,
      ),
      true,
    );
    assert.equal(
      controls.accept(input(2, { moveX: -1, actions: [{ type: "mine" }] }), 0, 10),
      true,
    );
    assert.equal(controls.command(1, 20)?.mine, true);
    assert.deepEqual(controls.ack, { inputSeq: 2, appliedTick: 1 });
    const second = controls.command(2, 30)!;
    assert.equal(second.moveX, -1);
    assert.equal(second.ammoSelection, "rocket");
    assert.equal(second.mine, false);
    assert.equal(controls.command(3, 40)?.mine, true);
    assert.equal(controls.command(4, 50)?.mine, false);
    assert.deepEqual(controls.ack, { inputSeq: 2, appliedTick: 1 });
    controls.accept(input(3, { actions: [{ type: "mine" }] }), 0, 100);
    controls.accept(input(4), 0, 300);
    assert.equal(controls.command(20, 350)?.mine, false, "new input cannot renew an old action");
  } finally {
    sim.dispose();
  }
});
test("validation rejects malformed, out-of-range, stale, duplicate and over-capacity messages atomically", () => {
  const { sim, controls, input } = setup();
  try {
    const malformed: unknown[] = [
      null,
      [],
      {},
      input(1, { moveX: NaN }),
      input(1, { moveZ: 2 }),
      input(1, { aim: { angle: Infinity } }),
      input(1, { aim: { x: 1025, z: 0 } }),
      input(1, { seq: 1.5 }),
      input(1, { controlEpoch: 0 }),
      input(1, { observedTick: 31 }),
      input(1, { observedTick: -1 }),
      input(1, { actions: [{ type: "ammo", weapon: "tow" }] }),
      { ...input(1), actions: [{ type: "ammo", weapon: "rocket", extra: true }] },
      { ...input(1), aim: { x: 1, z: 2, angle: 0 } },
    ];
    for (const data of malformed) assert.equal(controls.accept(data, 30, 0), false);
    assert.equal(controls.accept(input(1), 31, 0), false, "observed tick older than 500ms");
    assert.equal(
      controls.accept(
        input(1, { actions: Array.from({ length: 8 }, () => ({ type: "mine" })) }),
        0,
        0,
      ),
      true,
    );
    assert.equal(controls.accept(input(1), 0, 1), false);
    assert.equal(controls.accept(input(2, { actions: [{ type: "mine" }] }), 0, 1), false);
    assert.equal(controls.command(1, 1)?.mine, true);
    assert.equal(
      controls.accept(input(2, { actions: [{ type: "mine" }] }), 0, 2),
      true,
      "rejected input did not consume its sequence",
    );
    assert.deepEqual(controls.ack, { inputSeq: 1, appliedTick: 1 });
  } finally {
    sim.dispose();
  }
});
test("point aim is recomputed from authority and copied messages cannot mutate accepted controls", () => {
  const { sim, tank, controls, input } = setup();
  try {
    const packet = input(1, { aim: { x: 10, z: 10 } });
    controls.accept(packet, 0, 0);
    packet.moveX = -1;
    packet.actions.push({ type: "mine" });
    packet.aim = { angle: 0 };
    tank.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    assert.equal(controls.command(1, 10)?.aim, Math.PI / 4);
    tank.body.setTranslation({ x: 10, y: 0.65, z: 0 }, true);
    const command = controls.command(2, 20)!;
    assert.equal(command.aim, 0);
    assert.equal(command.moveX, 1);
    assert.equal(command.mine, false);
    controls.accept(input(2, { aim: { angle: -1 } }), 0, 30);
    assert.equal(controls.command(3, 40)?.aim, -1);
  } finally {
    sim.dispose();
  }
});
test("death, respawn, suspension and reconnect epochs discard old held input and queued actions", () => {
  const { sim, tank, controls, input } = setup();
  try {
    const old = input(1, { actions: [{ type: "mine" }] });
    controls.accept(old, 0, 0);
    tank.protection = 0;
    sim.damageTank(tank, 10000, -1, 1);
    const dead = controls.command(1, 10)!;
    assert.equal(dead.mine, false);
    assert.equal(dead.fire, false);
    assert.equal(controls.accept(old, 0, 10), false);
    const deadEpoch = controls.controlEpoch;
    sim.respawn(tank);
    assert.equal(controls.command(2, 20)?.mine, false);
    assert.ok(controls.controlEpoch > deadEpoch);
    controls.accept(input(1, { actions: [{ type: "mine" }] }), 0, 30);
    controls.suspend();
    const suspended = controls.controlEpoch;
    controls.suspend();
    assert.equal(controls.controlEpoch, suspended);
    assert.equal(controls.command(3, 40), undefined);
    controls.resume(50);
    assert.equal(controls.command(4, 50)?.mine, false);
    assert.equal(controls.command(5, 60)?.fire, false);
    assert.equal(controls.accept(old, 0, 60), false);
  } finally {
    sim.dispose();
  }
});
test("rate-limited traffic cannot extend an input lease", () => {
  const { sim, controls, input } = setup();
  try {
    for (let seq = 1; seq <= 60; seq++) assert.equal(controls.accept(input(seq), 0, 0), true);
    assert.equal(controls.accept(input(61), 0, 200), false);
    assert.equal(controls.command(15, 250)?.fire, false);
    assert.equal(controls.accept(input(61), 0, 1000), true);
    assert.equal(controls.command(60, 1000)?.fire, true);
  } finally {
    sim.dispose();
  }
});

test("human-only suspension and input timeout never enable AI, and resume clears old actions", () => {
  const { sim, tank } = setup();
  const controls = new PlayerControls(tank, 0, false);
  try {
    const packet = {
      controlEpoch: controls.controlEpoch,
      seq: 1,
      observedTick: 0,
      moveX: 1,
      moveZ: 0,
      aim: { angle: 0 },
      fire: true,
      actions: [{ type: "mine" }],
    };
    assert.equal(controls.accept(packet, 0, 0), true);
    controls.suspend();
    assert.equal(tank.driver, "idle");
    assert.equal(controls.accept(packet, 0, 1), false);
    sim.start();
    for (let i = 0; i < 30; i++)
      sim.stepWith(
        new Map([
          [
            tank.id,
            {
              moveX: 1,
              moveZ: 0,
              aim: 0,
              fire: true,
              mine: true,
            },
          ],
        ]),
      );
    assert.equal(tank.command.moveX, 0, "idle driver cannot consume stale commands");
    assert.equal(tank.command.fire, false);
    assert.equal(tank.command.mine, false);
    controls.resume(100);
    assert.equal(tank.driver, "human");
    assert.equal(controls.command(1, 100)?.fire, false);
    assert.equal(controls.command(1, 100)?.mine, false);
    controls.command(300, 5100);
    assert.equal(tank.driver, "idle", "silent human-only seats never hand control to AI");
  } finally {
    sim.dispose();
  }
});
test("wire input is rounded, omits idle defaults, and is accepted as the same command", () => {
  const { sim, controls, input } = setup();
  try {
    const idle = encodeInput(
      input(1, { moveX: 0, fire: false, aim: { x: 30.000_400_1, z: -40.123_456_7 } }),
    );
    assert.equal("fire" in idle, false);
    assert.equal("actions" in idle, false);
    assert.deepEqual(idle.aim, { x: 30, z: -40.123 });
    assert.equal(controls.accept(JSON.parse(JSON.stringify(idle)), 0, 0), true);
    assert.equal(controls.command(1, 0)?.fire, false);
    const edge = encodeInput(
      input(2, { moveX: -0.123_456, aim: { angle: Math.PI - 1e-6 }, actions: [{ type: "mine" }] }),
    );
    assert.equal(edge.moveX, -0.12);
    assert.deepEqual(edge.aim, { angle: Math.PI }, "rounding cannot push an angle past pi");
    assert.equal(controls.accept(JSON.parse(JSON.stringify(edge)), 1, 1), true);
    const command = controls.command(2, 1);
    assert.equal(command?.fire, true);
    assert.equal(command?.mine, true);
  } finally {
    sim.dispose();
  }
});
