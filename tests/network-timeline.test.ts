import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { createMultiplayerSimulation } from "../src/net/multiplayer-simulation";
import { captureRenderState, RenderTimeline } from "../src/net/render-timeline";
import { NetworkTimeline } from "../src/net/interpolation";
import type { Shot } from "../src/game/types";
before(async () => {
  await RAPIER.init();
});

test("local hull heading advances between packets through the short arc and resets on a new life", () => {
  const sim = createMultiplayerSimulation(4242, []);
  try {
    const source = captureRenderState(sim, sim.tanks[0].id);
    const pose = (heading: number, elapsed: number, life = 0) => {
      const viewer = { ...source.viewer, heading, life };
      return {
        ...source,
        viewer,
        elapsed,
        tanks: source.tanks.map((tank) => (tank.id === viewer.id ? viewer : tank)),
      };
    };
    const first = pose(3.1, 0),
      next = pose(-3.1, 0.05);
    const timeline = new RenderTimeline();
    timeline.reset({ state: first, events: [], ack: 0 });
    timeline.read(0, 0, 1 / 60, "extrapolate");
    timeline.push({ state: next, events: [], ack: 1 });
    const a = timeline.read(0.05, 0.05, 1 / 60, "extrapolate").state.viewer.heading;
    assert.ok(a > 3.1 && a < Math.PI * 2 - 3.1, "new packet must not snap local heading");
    const b = timeline.read(0.05, 0.0667, 1 / 60, "extrapolate").state.viewer.heading;
    assert.ok(b > a, "heading keeps moving while waiting for the next packet");
    assert.equal(next.viewer.heading, -3.1, "render smoothing must not mutate authority");
    const respawn = pose(-1, 0.1, 1);
    timeline.push({ state: respawn, events: [], ack: 2 });
    assert.equal(timeline.read(0.1, 0.1, 1 / 60, "extrapolate").state.viewer.heading, -1);
  } finally {
    sim.dispose();
  }
});

test("coalesced death and respawn preserve each life and emit effects on the display clock once", () => {
  const sim = createMultiplayerSimulation(4242, []);
  try {
    const first = captureRenderState(sim, sim.tanks[0].id);
    const deadTank = { ...first.viewer, alive: false, hp: 0 };
    const dead = {
      ...first,
      viewer: deadTank,
      tanks: first.tanks.map((t) => (t.id === deadTank.id ? deadTank : t)),
    };
    const nextLife = { ...first.viewer, life: 1, position: { x: 30, y: 0.65, z: 0 } };
    const respawn = {
      ...first,
      viewer: nextLife,
      tanks: first.tanks.map((t) => (t.id === nextLife.id ? nextLife : t)),
    };
    const timeline = new NetworkTimeline();
    timeline.reset(first, 0, 0);
    timeline.push(
      dead,
      2,
      [{ eventId: 1, tick: 2, event: { type: "death", id: deadTank.id, x: 0, z: 0 } }],
      [],
      0,
    );
    timeline.push(
      respawn,
      4,
      [{ eventId: 2, tick: 4, event: { type: "respawn", id: nextLife.id, x: 30, z: 0 } }],
      [],
      0,
    );
    timeline.push(respawn, 6, [], [], 0);
    assert.equal(timeline.read(17, 0, 1 / 60).state.viewer.alive, true);
    const death = timeline.read(34, 0, 1 / 60);
    assert.equal(death.state.viewer.alive, false);
    assert.deepEqual(
      death.events.map((e) => e.type),
      ["death"],
    );
    const alive = timeline.read(67, 0, 1 / 60);
    assert.equal(alive.state.viewer.life, 1);
    assert.equal(alive.state.viewer.position.x, 30);
    assert.deepEqual(
      alive.events.map((e) => e.type),
      ["respawn"],
    );
    assert.deepEqual(timeline.read(68, 0, 1 / 60).events, []);
    timeline.reset(respawn, 6, 70);
    assert.deepEqual(timeline.read(80, 0, 1 / 60).events, []);
  } finally {
    sim.dispose();
  }
});
test("a projectile born and destroyed between snapshots follows its swept segment and disappears at impact", () => {
  const sim = createMultiplayerSimulation(4242, []);
  try {
    const state = captureRenderState(sim, sim.tanks[0].id),
      timeline = new NetworkTimeline();
    const shot: Shot = {
      id: 999,
      x: 0,
      z: 0,
      y: 1,
      vx: 120,
      vz: 0,
      damage: 10,
      life: 1,
      owner: state.viewerId,
      ownerLife: 0,
      team: 0,
      weapon: "standard",
      piercing: 0,
      bounces: 0,
    };
    timeline.reset(state, 0, 0);
    timeline.push(
      state,
      6,
      [{ eventId: 1, tick: 1.5, event: { type: "impact", x: 1, z: 0 } }],
      [{ tick: 1, endTick: 1.5, shot, end: { x: 1, z: 0 } }],
      0,
    );
    assert.equal(timeline.read(10, 0, 1 / 60).state.shots.length, 0);
    const flight = timeline.read(20.8333333333, 0, 1 / 60);
    assert.ok(Math.abs(flight.state.shots[0].x - 0.5) < 1e-8);
    assert.deepEqual(flight.events, []);
    const impact = timeline.read(26, 0, 1 / 60);
    assert.equal(impact.state.shots.length, 0);
    assert.equal(impact.events[0].type, "impact");
    assert.equal(timeline.read(40, 0, 1 / 60).state.shots.length, 0);
  } finally {
    sim.dispose();
  }
});
test("moving debris rotations interpolate through the short quaternion arc without mutating history", () => {
  const sim = createMultiplayerSimulation(4242, []);
  try {
    const source = captureRenderState(sim, sim.tanks[0].id);
    const first = {
      ...source,
      elapsed: 0,
      covers: source.covers.map((cover, i) =>
        i ? cover : { ...cover, rotation: { x: 0, y: 0, z: 0, w: 1 } },
      ),
    };
    const last = {
      ...source,
      elapsed: 0.05,
      covers: source.covers.map((cover, i) =>
        i ? cover : { ...cover, rotation: { x: 0, y: 1, z: 0, w: 0 } },
      ),
    };
    const timeline = new RenderTimeline();
    timeline.reset({ state: first, events: [], ack: 0 });
    timeline.push({ state: last, events: [], ack: 1 });
    const rotation = timeline.read(0.025, 0.05, 1 / 60, "latest").state.covers[0].rotation;
    assert.ok(Math.abs(rotation.y - Math.SQRT1_2) < 1e-10);
    assert.ok(Math.abs(rotation.w - Math.SQRT1_2) < 1e-10);
    assert.equal(first.covers[0].rotation.y, 0);
  } finally {
    sim.dispose();
  }
});
