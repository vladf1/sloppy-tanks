import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { tankBurnout } from "../src/game/tank-destruction";
import { wreckModel } from "../src/game/wreck-model";

before(async () => {
  await RAPIER.init();
});

test("burnouts stay near one fifth of deaths with repeatable selection", () => {
  let count = 0;
  for (let id = 1; id <= 1000; id++) {
    if (tankBurnout(123, id, 1)) count++;
    assert.equal(tankBurnout(123, id, 1), tankBurnout(123, id, 1));
  }
  assert.ok(count > 160 && count < 240, String(count));
});

test("quiet kills keep one grounded wreck; dramatic kills retain launched pieces", () => {
  for (const quiet of [true, false]) {
    const sim = new Simulation(123);
    try {
      const tank = sim.human;
      tank.protection = 0;
      while (tankBurnout(sim.seed, tank.id, tank.deaths + 1) !== quiet) sim.seed++;
      const origin = tank.body.translation();
      sim.damageTank(tank, 9999, 999, tank.team === 0 ? 1 : 0);
      const death = sim.events.find((e) => e.type === "death")!;
      assert.equal(death.deathStyle === "burnout", quiet);
      if (quiet) {
        assert.equal(sim.fragments.length, 1);
        const wreck = sim.fragments[0];
        assert.equal(wreck.part, "intact");
        assert.equal(wreck.createdAt, sim.elapsed);
        assert.ok(wreck.body.linvel().y > 6 && wreck.body.linvel().y < 7.1);
        const spin = wreck.body.angvel();
        assert.ok(Math.hypot(spin.x, spin.z) > 0.5);
        const startY = wreck.body.translation().y;
        let peakY = startY;
        for (let i = 0; i < 120; i++) {
          sim.world.step();
          peakY = Math.max(peakY, wreck.body.translation().y);
        }
        assert.ok(peakY - startY > 0.8 && peakY - startY < 1.2, "hop stays around one metre");
        const p = wreck.body.translation();
        assert.ok(p.y < 1.5 && p.y > 0);
        assert.ok(Math.hypot(p.x - origin.x, p.z - origin.z) < 2);
        assert.ok(wreckModel(tank.kind, tank.team, "intact").children.length > 0);
      } else {
        assert.ok(sim.fragments.length >= 2);
        assert.ok(sim.fragments.some((f) => f.body.linvel().y > 5));
      }
      sim.reset();
      assert.equal(sim.fragments.length, 0);
    } finally {
      sim.world.free();
    }
  }
});

test("barrel detonation events carry their source without tagging shell blasts", () => {
  const sim = new Simulation(123);
  try {
    const barrel = sim.covers.find((c) => c.kind === "drum")!;
    sim.damageCover(barrel, 9999, sim.human.id, sim.human.team);
    assert.ok(sim.events.some((e) => e.type === "explosion" && e.coverKind === "drum"));
    sim.events.length = 0;
    sim.explode({ x: 50, z: 50 }, 0.1, 0, sim.human.id, sim.human.team);
    assert.equal(sim.events.find((e) => e.type === "explosion")?.coverKind, undefined);
  } finally {
    sim.world.free();
  }
});
