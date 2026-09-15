import RAPIER from "@dimforge/rapier3d-compat";
import { writeFileSync } from "node:fs";
import { Simulation } from "../src/game/simulation";

// Fixed 80-piece pool, twelve wrecked tanks, ten crates and sixteen concrete blocks.
// Repeated simultaneous blasts exercise existing wrecks without AI/render timing noise.
await RAPIER.init();
const runs = [];
for (let run = 0; run < 5; run++) {
  const sim = new Simulation(731);
  for (const c of sim.covers) sim.world.removeRigidBody(c.body);
  for (const t of sim.tanks) sim.world.removeRigidBody(t.body);
  sim.covers = [];
  sim.tanks = [];
  sim.coverByCollider.clear();
  sim.pickups = [];
  sim.movableCovers = [];
  for (let i = 0; i < 16; i++) {
    sim.addCover({
      kind: "teeth",
      x: (i % 4) * 6 - 9,
      z: Math.floor(i / 4) * 6 - 9,
      w: 2,
      h: 2,
      d: 2,
      hp: Infinity,
      color: 0xaaaaaa,
    });
  }
  for (let i = 0; i < 12; i++) {
    const tank = sim.addTank(1, false, "balanced", i);
    tank.protection = 0;
    tank.body.setTranslation({ x: (i % 4) * 4 - 6, y: 0.65, z: Math.floor(i / 4) * 4 - 4 }, true);
    sim.damageTank(tank, 1000, 999, 0);
  }
  sim.tanks = [];
  for (let i = 0; i < 10; i++) {
    const c = sim.addCover({
      kind: "cargo",
      x: (i % 5) * 4 - 8,
      z: Math.floor(i / 5) * 6 - 3,
      w: 2,
      h: 2,
      d: 2,
      hp: 40,
      color: 0x92734e,
    });
    sim.damageCover(c, 1000, 999, 0);
  }
  while (sim.fragments.length < sim.maxFragments) sim.fragment(0, 0, 0xaaaaaa, 0.5);
  sim.nav.rebuild(sim.covers);
  sim.start();
  const samples: number[] = [];
  const physics: number[] = [];
  const step = sim.world.step.bind(sim.world);
  sim.world.step = (...args) => {
    const start = performance.now();
    step(...args);
    physics.push(performance.now() - start);
  };
  let peakBodies = 0;
  for (let i = 0; i < 600; i++) {
    const start = performance.now();
    if (i % 60 === 0) for (const x of [-5, 0, 5]) sim.explode({ x, z: 0 }, 8, 80, 999, 0);
    sim.step();
    sim.events = [];
    samples.push(performance.now() - start);
    peakBodies = Math.max(peakBodies, sim.world.bodies.len());
  }
  const mean = (a: number[]) => a.reduce((x, y) => x + y, 0) / a.length;
  const p95 = (a: number[]) => a.sort((x, y) => x - y)[Math.floor(a.length * 0.95)];
  let sleeping = 0;
  let dynamic = 0;
  sim.world.bodies.forEach((body) => {
    if (body.isDynamic()) {
      dynamic++;
      if (body.isSleeping()) sleeping++;
    }
  });
  runs.push({
    simulationMean: mean(samples),
    simulationP95: p95(samples),
    physicsMean: mean(physics),
    physicsP95: p95(physics),
    peakBodies,
    fragments: sim.fragments.length,
    dynamic,
    sleeping,
  });
  sim.dispose();
}
const output = process.argv[2] ?? "artifacts/destruction-benchmark.json";
writeFileSync(output, JSON.stringify({ seed: 731, stepsPerRun: 600, runs }, null, 2));
console.log(JSON.stringify(runs, null, 2));
