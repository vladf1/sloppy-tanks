import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { humveeTumble, tankBurnout } from "../src/game/tank-destruction";
import { breakTank } from "../src/game/wrecks";
import { Random } from "../src/game/data";
import type { Team } from "../src/game/types";
import { clearArena, placeTank } from "./fixtures";

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
      while (tankBurnout(sim.seed, tank.id, tank.life + 1) !== quiet) sim.seed++;
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
      } else {
        assert.ok(sim.fragments.length >= 2);
        assert.ok(sim.fragments.some((f) => f.body.linvel().y > 5));
      }
      sim.reset();
      assert.equal(sim.fragments.length, 0);
    } finally {
      sim.dispose();
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
    sim.dispose();
  }
});

test("tank breakup varies assemblies, travels widely and lands after flight", () => {
  const variants = new Set<string>();
  const axes = new Set<string>();
  let highLaunches = 0;
  let highest = 0;
  // Seeds chosen to cover a high turret launch, both assemblies and all three tumble axes.
  const seeds = [3, 7, 8];
  for (const seed of seeds) {
    const s = new Simulation(123);
    try {
      const t = s.human;
      clearArena(s, [t]);
      s.start();
      t.protection = 0;
      placeTank(t, 0, 0);
      s.world.step();
      s.rng = new Random(seed);
      s.damageTank(t, 1000, 999, (1 - t.team) as Team);
      const pieces = [...s.fragments];
      const names = pieces.map((f) => f.part).sort();
      assert.ok(names.includes("hull"));
      assert.ok(
        names.includes("turret-barrel") || (names.includes("turret") && names.includes("barrel")),
      );
      variants.add(names.join("/"));
      assert.ok(pieces.length <= 3);
      if (pieces[1].body.linvel().y ** 2 / 44 >= 20) highLaunches++;
      assert.ok(pieces[0].body.linvel().y ** 2 / 44 <= 8.01, "hulls keep normal arcs");
      for (const piece of pieces) {
        const v = piece.body.angvel();
        const speed = Math.hypot(v.x, v.y, v.z);
        assert.ok(speed >= 6.99 && speed <= 14.01);
        axes.add(Object.entries(v).sort((a, b) => Math.abs(b[1]) - Math.abs(a[1]))[0][0]);
      }
      const targets = pieces.slice(0, 2).map((piece) => {
        const p = piece.body.translation();
        const v = piece.body.linvel();
        const flight = (v.y + Math.sqrt(v.y ** 2 + 44 * Math.max(0, p.y - 0.3))) / 22;
        return { x: p.x + v.x * flight, z: p.z + v.z * flight };
      });
      assert.ok(
        Math.hypot(targets[0].x - targets[1].x, targets[0].z - targets[1].z) > 12,
        "launch aims hull and turret several tank lengths apart; later contacts may deflect them",
      );
      const landingSteps = Math.ceil(Math.max(...pieces.map((f) => f.life - 3.2)) * 60) + 60;
      for (let i = 0; i < landingSteps; i++) {
        s.world.step();
        highest = Math.max(highest, ...pieces.map((f) => f.body.translation().y));
      }
      assert.ok(
        pieces.every((f) => f.body.translation().y < 2),
        "parts land after their ballistic flight, including high launches",
      );
    } finally {
      s.dispose();
    }
  }
  assert.equal(variants.size, 2);
  assert.ok(highest > 20, "some turrets take spectacular high arcs");
  assert.ok(highLaunches > 0 && highLaunches < seeds.length, "high launches are occasional");
  assert.equal(axes.size, 3, "tumbling varies across all three axes");
});

test("deaths during a full fragment burst remain within the shared debris and wreck cap", () => {
  const s = new Simulation(123);
  try {
    s.start();
    for (const t of s.tanks) t.protection = 0;
    for (let i = 0; i < 150; i++) s.fragment(0, 0, 0, 0.5);
    assert.equal(s.fragments.length, s.maxFragments);
    for (const t of s.tanks) {
      s.damageTank(t, 1000, 999, (1 - t.team) as Team);
      assert.ok(s.fragments.length <= s.maxFragments);
    }
  } finally {
    s.dispose();
  }
});

test("Humvee deaths alternate between quiet burnouts and bounded whole-vehicle tumbles", () => {
  for (const quiet of [false, true]) {
    const simulation = new Simulation(123);
    try {
      const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
      const attacker = simulation.tanks.find((tank) => tank.team !== hunter.team)!;
      hunter.protection = 0;
      while (tankBurnout(simulation.seed, hunter.id, hunter.life + 1) !== quiet) simulation.seed++;
      const bodies = simulation.world.bodies.len();
      simulation.damageTank(hunter, 1000, attacker.id, attacker.team);
      const death = simulation.events.find(
        (event) => event.type === "death" && event.id === hunter.id,
      )!;
      assert.ok(death);
      assert.equal(death.deathStyle === "burnout", quiet);
      const pieces = simulation.fragments.filter((fragment) => fragment.wreck === "humvee");
      assert.equal(pieces.length, 1);
      assert.equal(pieces[0].part, "intact");
      const lift = pieces[0].body.linvel().y;
      const spin = Math.hypot(...Object.values(pieces[0].body.angvel()));
      assert.ok(quiet ? lift > 6 && lift < 7.1 : lift > 4 && lift < 11);
      assert.ok(quiet ? spin < 1 : spin > 3 && spin < 7);
      assert.equal(simulation.world.bodies.len(), bodies);
    } finally {
      simulation.dispose();
    }
  }
});

test("Humvee low rolls settle on a side at either heading without consuming combat RNG", () => {
  for (const heading of [0, Math.PI / 2]) {
    const simulation = new Simulation(2);
    const control = new Simulation(2);
    try {
      const hunter = simulation.tanks.find((tank) => tank.kind === "humvee")!;
      for (const cover of simulation.covers) simulation.world.removeRigidBody(cover.body);
      for (const tank of simulation.tanks)
        if (tank !== hunter) simulation.world.removeRigidBody(tank.body);
      hunter.heading = heading;
      hunter.body.setTranslation({ x: 0, y: 0.65, z: 0 }, true);
      hunter.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
      breakTank(simulation, hunter);
      assert.equal(simulation.rng.next(), control.rng.next());
      const wreck = simulation.fragments.find((fragment) => fragment.wreck === "humvee")!;
      for (let i = 0; i < 300; i++) simulation.world.step();
      const q = wreck.body.rotation();
      const sideUp = 2 * (q.x * q.y + q.w * q.z);
      assert.ok(Math.abs(sideUp) > 0.95, `expected side landing, got ${sideUp}`);
    } finally {
      simulation.dispose();
      control.dispose();
    }
  }
});

test("Humvee tumble selection varies axes and direction reproducibly", () => {
  const motions = Array.from({ length: 64 }, (_, seed) => humveeTumble(seed, 9, 1));
  assert.equal(new Set(motions.map((motion) => motion.height)).size, 4);
  assert.ok(motions.some((motion) => motion.roll > 0));
  assert.ok(motions.some((motion) => motion.roll < 0));
  assert.deepEqual(humveeTumble(12, 9, 1), humveeTumble(12, 9, 1));
});
