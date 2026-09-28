import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-simd-compat";
import { StateMirror, StateStream, type Snapshot } from "../src/net/replication";
import { captureScene, projectScene } from "../src/net/scene-codec";
import { createMultiplayerSimulation } from "../src/net/multiplayer-simulation";
import { idleCommand } from "../src/game/types";
import { DEBRIS_CLEANUP_SECONDS } from "../src/game/debris-cleanup";

before(async () => {
  await RAPIER.init();
});

for (const mapMode of ["village", "harbor", "quarry"] as const) {
  test(
    mapMode +
      ": full and field deltas round-trip through JSON, including destruction and late joins",
    () => {
      const sim = createMultiplayerSimulation(
        4242,
        [{ playerId: "one", name: "One", team: 0, slot: 0, kind: "balanced" }],
        { mapMode },
      );
      try {
        sim.start();
        const identity = { roomEpoch: "room", roundId: 1 };
        const stream = new StateStream(identity),
          mirror = new StateMirror();
        const full = stream.full(captureScene(sim), 0, 0);
        assert.ok(Buffer.byteLength(JSON.stringify(full)) < 160_000, "Full-state wire budget");
        mirror.applyFull(JSON.parse(JSON.stringify(full)), identity);
        let removed = false;
        for (let tick = 1; tick <= 180; tick++) {
          if (tick === 30)
            for (const cover of sim.covers
              .filter((cover) => cover.alive && cover.destructible)
              .slice(0, 8))
              sim.damageCover(cover, 10000, -1, 0);
          if (tick === 90) for (const fragment of sim.fragments) fragment.life = 0;
          sim.stepWith(new Map([[sim.human.id, { ...idleCommand(), moveX: 1, fire: true }]]));
          sim.events = [];
          if (tick % 3) continue;
          const scene = captureScene(sim),
            snap = stream.snapshot(scene, tick, [], []);
          assert.ok(
            Buffer.byteLength(JSON.stringify(snap)) < 128_000,
            "Burst snapshot wire budget",
          );
          removed ||= !!snap.removed;
          assert.ok(mirror.applySnapshot(JSON.parse(JSON.stringify(snap))));
          assert.deepEqual(mirror.state, scene);
          assert.deepEqual(mirror.render(sim.human.id), projectScene(scene, sim.human.id));
          if (tick === 60 || tick === 93) {
            const late = new StateMirror();
            late.applyFull(JSON.parse(JSON.stringify(stream.full(scene, tick, 0))), identity);
            assert.deepEqual(late.render(sim.human.id), mirror.render(sim.human.id));
          }
        }
        assert.equal(removed, true);
        assert.ok(mirror.render(sim.human.id).covers.some((cover) => cover.maxHp === Infinity));
        assert.doesNotMatch(JSON.stringify(mirror.state), /"body"|"collider"|Infinity|NaN/);
      } finally {
        sim.dispose();
      }
    },
  );
}
test("frames omit identity and unchanged data, and scenes carry only presentation fields", () => {
  const sim = createMultiplayerSimulation(
    4242,
    [{ playerId: "one", name: "One", team: 0, slot: 0, kind: "balanced" }],
    { mapMode: "village" },
  );
  try {
    sim.start();
    sim.stepWith(new Map([[sim.human.id, { ...idleCommand(), fire: true }]]));
    const scene = captureScene(sim);
    const stream = new StateStream({ roomEpoch: "room", roundId: 1 });
    stream.full(scene, 0, 0);
    assert.deepEqual(Object.keys(stream.snapshot(scene, 1, [], [])), ["seq", "tick", "elapsed"]);
    const { entities } = scene;
    assert.ok(entities.shots.length && entities.covers.some((cover) => cover.motion));
    for (const shot of entities.shots)
      assert.deepEqual(
        Object.keys(shot).filter(
          (field) =>
            !["id", "x", "z", "y", "visualY", "vx", "vz", "weapon", "team"].includes(field),
        ),
        [],
      );
    for (const cover of entities.covers)
      if (cover.motion)
        assert.deepEqual(Object.keys(cover.motion), ["originX", "originZ", "w", "d"]);
    assert.ok(entities.tanks.every((tank) => !("previous" in tank)));
    const viewer = projectScene(scene, sim.human.id).viewer;
    assert.deepEqual(viewer.previous, { x: viewer.position.x, z: viewer.position.z });
  } finally {
    sim.dispose();
  }
});
test("debris life reaches clients only once its final fade begins", () => {
  const sim = createMultiplayerSimulation(
    4242,
    [{ playerId: "one", name: "One", team: 0, slot: 0, kind: "balanced" }],
    { mapMode: "village" },
  );
  try {
    sim.fragment(0, 0, 0xffffff);
    const piece = sim.fragments[0];
    const lifeUpdate = (snapshot: Snapshot) => snapshot.updates?.fragments?.[piece.id]?.life;
    piece.life = 5;
    const stream = new StateStream({ roomEpoch: "room", roundId: 1 });
    stream.full(captureScene(sim), 0, 0);
    piece.life = 4;
    assert.equal(lifeUpdate(stream.snapshot(captureScene(sim), 1, [], [])), undefined);
    piece.life = DEBRIS_CLEANUP_SECONDS / 2;
    assert.equal(
      lifeUpdate(stream.snapshot(captureScene(sim), 2, [], [])),
      DEBRIS_CLEANUP_SECONDS / 2,
    );
  } finally {
    sim.dispose();
  }
});

test("mirror rejects corrupt or skipped deltas atomically and a full baseline repairs it", () => {
  const sim = createMultiplayerSimulation(4242, []);
  try {
    const identity = { roomEpoch: "r", roundId: 1 },
      state = captureScene(sim),
      stream = new StateStream(identity),
      mirror = new StateMirror();
    const full = stream.full(state, 0, 0);
    mirror.applyFull(full, identity);
    const before = JSON.stringify(mirror.state),
      bad: Snapshot = {
        ...stream.snapshot(state, 3, [], []),
        updates: { tanks: { [sim.tanks[0].id]: { hp: "bad" } } },
      };
    assert.equal(mirror.applySnapshot(bad), undefined);
    assert.equal(JSON.stringify(mirror.state), before);
    assert.equal(mirror.needsFull, true);
    mirror.applyFull(full, identity);
    assert.equal(mirror.applySnapshot({ ...bad, seq: 9 }), undefined);
    assert.equal(JSON.stringify(mirror.state), before);
    mirror.applyFull(stream.full(state, 3, 0), identity);
    assert.equal(mirror.needsFull, false);
  } finally {
    sim.dispose();
  }
});

test("mirror allows one change per entity of each kind in a frame", () => {
  const sim = createMultiplayerSimulation(4242, []);
  try {
    const identity = { roomEpoch: "r", roundId: 1 },
      state = captureScene(sim),
      mirror = new StateMirror();
    const full = new StateStream(identity).full(state, 0, 0);
    mirror.applyFull(full, identity);
    const before = JSON.stringify(mirror.state),
      tankId = sim.tanks[0].id,
      frame = { tick: 1, elapsed: state.elapsed },
      mine = { id: tankId, x: 0, z: 0, owner: tankId, team: 0, arm: 0, life: 1 };
    assert.equal(
      mirror.applySnapshot({
        ...frame,
        seq: 1,
        updates: { tanks: { [tankId]: { hp: 1 } } },
        removed: { tanks: [tankId] },
      }),
      undefined,
    );
    assert.equal(JSON.stringify(mirror.state), before);
    assert.equal(mirror.needsFull, true);
    mirror.applyFull(full, identity);
    // Ids are claimed per kind, so another kind may reuse a tank's id in the same frame.
    assert.ok(
      mirror.applySnapshot({
        ...frame,
        seq: 1,
        updates: { tanks: { [tankId]: { hp: 1 } }, mines: { [tankId]: mine } },
      }),
    );
    assert.equal(mirror.state?.entities.tanks[0].hp, 1);
    assert.deepEqual(mirror.state?.entities.mines.at(-1), mine);
    // The removal that conflicted with the update above is valid in a frame of its own.
    assert.ok(mirror.applySnapshot({ ...frame, seq: 2, removed: { tanks: [tankId] } }));
    assert.equal(
      mirror.state?.entities.tanks.some((tank) => tank.id === tankId),
      false,
    );
  } finally {
    sim.dispose();
  }
});
