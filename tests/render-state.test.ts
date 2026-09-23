import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { renderState } from "../src/game/render-state";
import { captureRenderState, RenderTimeline } from "../src/net/render-timeline";
before(async () => {
  await RAPIER.init();
});

test("local presentation views stay live and reuse arrays without exposing physics handles", () => {
  const simulation = new Simulation(4242);
  try {
    const view = renderState(simulation);
    const tanks = view.tanks;
    const human = view.viewer;
    const before = human.position;
    simulation.human.body.setTranslation({ x: before.x + 1, y: before.y, z: before.z }, true);
    assert.equal(renderState(simulation), view);
    assert.equal(view.tanks, tanks);
    assert.equal(view.viewer, human);
    assert.equal(human.position.x, before.x + 1);
    for (const entity of [...view.tanks, ...view.covers, ...view.fragments]) {
      assert.equal("body" in entity, false);
      assert.equal("collider" in entity, false);
    }
    simulation.reset();
    assert.equal(view.tanks, tanks);
    assert.notEqual(view.viewer, human, "reset must replace cached entity identity");
  } finally {
    simulation.dispose();
  }
});

test("delayed display retains remote tanks until their death and emits the effect once", () => {
  const simulation = new Simulation(4242);
  try {
    simulation.start();
    const first = captureRenderState(simulation);
    const victim = simulation.tanks.find((tank) => !tank.human)!;
    const atStart = first.tanks.find((tank) => tank.id === victim.id)!;
    victim.protection = 0;
    simulation.damageTank(victim, 10000, simulation.human.id, simulation.human.team);
    simulation.elapsed = 0.05;
    const events = simulation.events.splice(0).filter((event) => event.type === "death");
    assert.ok(events.length);
    const last = captureRenderState(simulation);
    const timeline = new RenderTimeline();
    timeline.reset({ state: first, events: [], ack: 0 });
    timeline.push({ state: last, events, ack: 1 });
    const beforeDeath = timeline.read(0.025, 0.05, 1 / 60, "latest");
    assert.equal(beforeDeath.state.tanks.find((tank) => tank.id === victim.id)!.alive, true);
    assert.deepEqual(beforeDeath.events, []);
    const afterDeath = timeline.read(0.05, 0.05, 1 / 60, "latest");
    assert.equal(afterDeath.state.tanks.find((tank) => tank.id === victim.id)!.alive, false);
    assert.deepEqual(afterDeath.events, events);
    assert.deepEqual(timeline.read(0.06, 0.06, 1 / 60, "latest").events, []);
    assert.equal(atStart.alive, true, "render interpolation must never mutate history");
  } finally {
    simulation.dispose();
  }
});

test("local extrapolation is bounded and resets across tank lives", () => {
  const simulation = new Simulation(4242);
  try {
    simulation.human.body.setLinvel({ x: 10, y: 0, z: 0 }, true);
    const first = captureRenderState(simulation);
    const timeline = new RenderTimeline();
    timeline.reset({ state: first, events: [], ack: 0 });
    assert.ok(
      Math.abs(
        timeline.read(0, 5, 1 / 60, "extrapolate").state.viewer.position.x -
          first.viewer.position.x -
          1,
      ) < 1e-6,
    );
    simulation.human.life++;
    simulation.human.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
    simulation.human.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
    simulation.elapsed = 0.05;
    timeline.push({ state: captureRenderState(simulation), events: [], ack: 1 });
    assert.equal(timeline.read(0.05, 0.05, 1 / 60, "smooth").state.viewer.position.x, 0);
  } finally {
    simulation.dispose();
  }
});
