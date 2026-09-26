import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { renderState } from "../src/game/render-state";
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
