import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { placeMine, stepMines, stepProjectiles } from "../src/game/weapons";
import { clearArena, placeTank } from "./fixtures";
import { STEP } from "../src/game/data";
before(async () => {
  await RAPIER.init();
});
function arena() {
  const s = clearArena(new Simulation(123));
  s.mines = [];
  s.events = [];
  return s;
}
/** A team-0 layer and a team-1 victim `gap` metres east of it. */
function duel(layerX: number, gap: number) {
  const s = new Simulation(123);
  const layer = s.tanks.find((t) => t.team === 0)!;
  const victim = s.tanks.find((t) => t.team === 1)!;
  clearArena(s, [layer, victim]);
  s.start();
  for (const t of s.tanks) t.protection = 0;
  placeTank(layer, layerX, 0);
  placeTank(victim, layerX + gap, 0);
  s.world.step();
  return { s, layer, victim };
}
function shell(s: Simulation, x = 0) {
  s.shots.push({
    id: s.nextId++,
    x,
    z: -5,
    vx: 0,
    vz: 600,
    damage: 40,
    bounces: 0,
    life: 1,
    piercing: 0,
    weapon: "standard",
    owner: 999,
    team: 0,
  });
}
test("fast shells detonate enemy and friendly mines, armed or arming, exactly once", () => {
  for (const team of [0, 1] as const)
    for (const arm of [0, 0.8]) {
      const s = arena();
      s.mines.push({ id: s.nextId++, x: 0, z: 0, owner: 5, team, arm, life: 25 });
      shell(s);
      stepProjectiles(s, STEP);
      assert.equal(s.mines.length, 0);
      assert.equal(s.shots.length, 0);
      assert.equal(s.events.filter((e) => e.type === "explosion").length, 1);
      s.dispose();
    }
});
test("cover and clean misses protect mines while grazing hits register", () => {
  for (const scenario of ["cover", "miss", "graze"] as const) {
    const s = arena();
    s.mines.push({ id: s.nextId++, x: 0, z: 0, owner: 5, team: 1, arm: 0, life: 25 });
    if (scenario === "cover")
      s.addCover({ kind: "concrete", x: 0, z: -2, w: 3, d: 0.3, h: 3, hp: Infinity, color: 0 });
    s.world.step();
    shell(s, scenario === "miss" ? 0.8 : scenario === "graze" ? 0.6 : 0);
    stepProjectiles(s, STEP);
    assert.equal(s.mines.length, scenario === "graze" ? 0 : 1, scenario);
    s.dispose();
  }
});
test("shooting a mine chains nearby mines once and credits the shooter", () => {
  const s = arena();
  s.start();
  s.addTank(1, true, "balanced");
  const target = s.tanks[0];
  target.body.setTranslation({ x: 3, y: 0.65, z: 0 }, true);
  target.protection = 0;
  target.hp = 10;
  for (const x of [0, 1])
    s.mines.push({ id: s.nextId++, x, z: 0, owner: 5, team: 1, arm: 0.8, life: 25 });
  shell(s);
  stepProjectiles(s, STEP);
  assert.equal(s.mines.length, 0);
  assert.equal(s.events.filter((e) => e.type === "explosion").length, 2);
  assert.equal(target.alive, false);
  assert.equal(s.match.scores[0], 1);
  s.dispose();
});

test("mines arm after delay, ignore allies, preserve original owner", () => {
  const { s, layer, victim } = duel(0, 1);
  victim.hp = 40;
  placeMine(s, layer);
  assert.equal(s.mines.length, 1);
  stepMines(s, 0.5);
  assert.ok(victim.alive);
  assert.equal(s.mines.length, 1);
  stepMines(s, 0.4);
  assert.equal(s.mines.length, 0);
  assert.equal(victim.alive, false);
  assert.equal(s.match.scores[0], 1);
  s.dispose();
});
test("chain-triggered mines are removed safely during mine iteration", () => {
  const { s, layer, victim } = duel(-20, 21);
  victim.hp = 20;
  for (const x of [0, 1, 2, 3])
    s.mines.push({ id: s.nextId++, x, z: 0, owner: layer.id, team: layer.team, arm: 0, life: 25 });
  stepMines(s, STEP);
  assert.equal(s.mines.length, 0);
  assert.equal(s.match.scores[0], 1);
  s.dispose();
});
