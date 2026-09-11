import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { AMMO_ORDER, emptyAmmo, isSpecialAmmo, selectAmmo } from "../src/game/ammunition";
import { collectPickup, fireWeapon, stepProjectiles } from "../src/game/weapons";
import { WEAPONS, STEP, distance } from "../src/game/data";
import {
  idleCommand,
  type Shot,
  type Weapon,
  type Team,
  type SpecialAmmo,
} from "../src/game/types";
import { BOT_AMMO, preferredAmmo } from "../src/game/bot-personalities";
import { botCommand } from "../src/game/ai";
import { spawnPositions } from "../src/game/arena";
before(async () => {
  await RAPIER.init();
});

function arena(count = 1) {
  const s = new Simulation(123);
  for (const c of s.covers) s.world.removeRigidBody(c.body);
  s.covers = [];
  s.coverByCollider.clear();
  s.pickups = [];
  s.nav.rebuild([]);
  for (const t of s.tanks.slice(count)) s.world.removeRigidBody(t.body);
  s.tanks = s.tanks.slice(0, count);
  for (const [i, t] of s.tanks.entries()) {
    t.human = true;
    t.protection = 0;
    t.aim = 0;
    t.body.setTranslation({ x: 0, y: 0.65, z: i * 16 }, true);
    t.previous = { x: 0, z: i * 16 };
  }
  s.world.step();
  s.start();
  return s;
}
function crate(s: Simulation, kind: SpecialAmmo) {
  return { id: s.nextId++, kind, x: 0, z: 0, available: true, cooldown: 0 };
}
function shot(s: Simulation, weapon: Weapon, x: number, vx: number, team: Team): Shot {
  return {
    id: s.nextId++,
    weapon,
    x,
    z: 0,
    vx,
    vz: 0,
    team,
    owner: 999 + team,
    damage: WEAPONS[weapon].damage,
    bounces: 0,
    life: 3.5,
    piercing: weapon === "piercing" ? 1 : 0,
  };
}

for (const weapon of AMMO_ORDER)
  test(`${weapon} emits the correct shot, costs one unit, and respects cooldown`, () => {
    const s = arena(),
      t = s.human;
    if (isSpecialAmmo(weapon)) collectPickup(s, t, crate(s, weapon));
    selectAmmo(t, weapon);
    const before = { ...t.ammo };
    fireWeapon(s, t);
    assert.equal(s.shots.length, weapon === "spread" ? 3 : 1);
    const event = s.events.find((e) => e.type === "shot")!;
    assert.equal(event.weapon, weapon);
    assert.equal(event.id, t.id);
    for (const p of s.shots) {
      assert.equal(p.weapon, weapon);
      assert.equal(p.damage, WEAPONS[weapon].damage);
      assert.equal(p.bounces, WEAPONS[weapon].bounces);
      assert.equal(p.piercing, weapon === "piercing" ? 1 : 0);
      assert.ok(Math.abs(Math.hypot(p.vx, p.vz) - WEAPONS[weapon].speed) < 1e-9);
    }
    if (isSpecialAmmo(weapon)) assert.equal(t.ammo[weapon], before[weapon] - 1);
    else assert.deepEqual(t.ammo, emptyAmmo());
    const after = { ...t.ammo },
      cooldown = t.cooldown,
      count = s.shots.length;
    fireWeapon(s, t);
    assert.equal(s.shots.length, count);
    assert.deepEqual(t.ammo, after);
    assert.equal(t.cooldown, cooldown);
    t.cooldown = 0;
    t.rapid = 12;
    fireWeapon(s, t);
    assert.equal(t.cooldown, WEAPONS[weapon].interval / 2 / 1.2);
    s.dispose();
  });

test("standard remains unlimited over sustained firing", () => {
  const s = arena(),
    t = s.human;
  for (let i = 0; i < 500; i++) {
    t.cooldown = 0;
    fireWeapon(s, t);
    s.shots = [];
  }
  assert.equal(s.shotsFired, 500);
  assert.deepEqual(t.ammo, emptyAmmo());
  assert.equal(t.selectedAmmo, "standard");
  s.dispose();
});

test("selection cycles both directions, skips empty slots, wraps, and never changes cooldown", () => {
  const s = arena(),
    t = s.human;
  t.cooldown = 0.72;
  selectAmmo(t, 1);
  assert.equal(t.selectedAmmo, "standard");
  t.ammo.spread = 2;
  t.ammo.piercing = 1;
  for (const expected of ["spread", "piercing", "standard"]) {
    selectAmmo(t, 1);
    assert.equal(t.selectedAmmo, expected);
  }
  for (const expected of ["piercing", "spread", "standard"]) {
    selectAmmo(t, -1);
    assert.equal(t.selectedAmmo, expected);
  }
  selectAmmo(t, "rocket");
  assert.equal(t.selectedAmmo, "standard");
  assert.equal(t.cooldown, 0.72);
  s.dispose();
});

test("selection precedes held fire, depletion falls back, and switching cannot bypass reload", () => {
  const s = arena(),
    t = s.human;
  t.ammo.rocket = 1;
  t.ammo.spread = 2;
  s.step({ ...idleCommand(), fire: true, ammoSelection: "rocket" });
  assert.equal(s.shots[0].weapon, "rocket");
  assert.equal(t.ammo.rocket, 0);
  assert.equal(t.selectedAmmo, "standard");
  const cooldown = t.cooldown;
  for (const ammoSelection of ["spread", "standard", "spread"] as const)
    s.step({ ...idleCommand(), fire: true, ammoSelection });
  assert.equal(s.shotsFired, 1);
  assert.equal(t.ammo.spread, 2);
  assert.ok(Math.abs(t.cooldown - (cooldown - 3 * STEP)) < 1e-9);
  selectAmmo(t, "standard");
  while (s.shotsFired === 1) s.step({ ...idleCommand(), fire: true });
  assert.equal(s.shots.at(-1)!.weapon, "standard");
  assert.equal(t.cooldown, WEAPONS.standard.interval / 1.2);
  s.dispose();
});

test("death, respawn and reset clear inventories; snapshots own their inventory copy", () => {
  const s = arena(),
    t = s.human;
  for (const w of AMMO_ORDER.filter(isSpecialAmmo)) collectPickup(s, t, crate(s, w));
  selectAmmo(t, "piercing");
  const snapshot = s.snapshot().tanks[0];
  assert.equal(snapshot.selectedAmmo, "piercing");
  s.damageTank(t, 999, t.id, t.team);
  assert.deepEqual(t.ammo, emptyAmmo());
  assert.equal(t.selectedAmmo, "standard");
  assert.equal(s.pickups.length, 0);
  assert.equal(snapshot.ammo.piercing, 24);
  t.ammo.spread = 1;
  selectAmmo(t, 1);
  assert.equal(t.selectedAmmo, "standard");
  s.respawn(t);
  assert.deepEqual(t.ammo, emptyAmmo());
  t.ammo.rocket = 7;
  t.selectedAmmo = "rocket";
  s.reset();
  assert.ok(s.tanks.every((t) => t.selectedAmmo === "standard"));
  assert.ok(s.tanks.every((t) => Object.values(t.ammo).every((n) => n === 0)));
  s.dispose();
});

test("paused and results simulations ignore selection", () => {
  const s = arena(),
    t = s.human;
  t.ammo.spread = 1;
  for (const phase of ["ready", "paused", "results"] as const) {
    s.match.phase = phase;
    s.step({ ...idleCommand(), ammoSelection: 1, fire: true });
    assert.equal(t.selectedAmmo, "standard");
    assert.equal(t.ammo.spread, 1);
  }
  s.dispose();
});

for (const kind of AMMO_ORDER.filter(isSpecialAmmo))
  test(`${kind} crates equip the first advanced ammo and report actual receipt without clearing cooldown`, () => {
    const s = arena(),
      t = s.human;
    t.cooldown = 0.6;
    const p = crate(s, kind);
    assert.equal(collectPickup(s, t, p), true);
    assert.equal(t.ammo[kind], WEAPONS[kind].perCrate);
    assert.equal(t.selectedAmmo, kind);
    assert.equal(t.cooldown, 0.6);
    assert.equal(p.cooldown, 13);
    t.ammo[kind] = WEAPONS[kind].carryLimit - 3;
    assert.equal(collectPickup(s, t, crate(s, kind)), true);
    assert.equal(t.ammo[kind], WEAPONS[kind].carryLimit);
    assert.equal(s.events.at(-1)!.label, `+3 ${WEAPONS[kind].unit}`);
    const full = crate(s, kind),
      count = s.events.length;
    assert.equal(collectPickup(s, t, full), false);
    assert.equal(full.available, true);
    assert.equal(full.cooldown, 0);
    assert.equal(s.events.length, count);
    s.dispose();
  });

test("collecting another ammo type preserves selection when advanced ammo is already stocked", () => {
  const s = arena(),
    t = s.human;
  collectPickup(s, t, crate(s, "spread"));
  selectAmmo(t, "standard");
  collectPickup(s, t, crate(s, "rocket"));
  assert.equal(t.selectedAmmo, "standard");
  assert.equal(t.ammo.spread, WEAPONS.spread.perCrate);
  assert.equal(t.ammo.rocket, WEAPONS.rocket.perCrate);
  s.dispose();
});

test("simultaneous collection skips full tanks, awards one recipient, and refills after 13 seconds", () => {
  const s = arena(3),
    [full, first, second] = s.tanks;
  for (const [i, t] of s.tanks.entries()) {
    t.body.setTranslation({ x: (i - 1) * 0.5, y: 0.65, z: 0 }, true);
    // Disable physical separation for this single shared pickup-contact fixture.
    for (let j = 0; j < t.body.numColliders(); j++) t.body.collider(j).setCollisionGroups(0);
  }
  full.ammo.rocket = 24;
  const p = crate(s, "rocket");
  s.pickups.push(p);
  s.step();
  assert.deepEqual([full.ammo.rocket, first.ammo.rocket, second.ammo.rocket], [24, 12, 0]);
  assert.equal(s.events.filter((e) => e.type === "pickup").length, 1);
  for (const t of s.tanks) t.body.setTranslation({ x: 20, y: 0.65, z: 20 }, true);
  for (let i = 0; i < 779; i++) s.step();
  assert.equal(p.available, false);
  for (let i = 0; i < 2; i++) s.step();
  assert.equal(p.available, true);
  s.dispose();
});

for (const opposing of ["standard", "rocket", "piercing"] as const)
  for (const reversed of [false, true])
    test(`piercing intercepts ${opposing}, order=${reversed}`, () => {
      const s = arena(0),
        a = shot(s, "piercing", -2, 20, 0),
        b = shot(s, opposing, 2, -20, 1);
      s.shots = reversed ? [b, a] : [a, b];
      stepProjectiles(s, 0.081);
      assert.equal(s.shots.length, opposing === "piercing" ? 2 : 1);
      assert.equal(a.piercing, 0);
      if (opposing === "piercing") assert.equal(b.piercing, 0);
      assert.equal(s.events.filter((e) => e.type === "impact").length, 1);
      assert.equal(s.events.filter((e) => e.type === "explosion").length, 0);
      // Start the next tick still within interception radius; do not resolve this pair twice.
      stepProjectiles(s, 0.01);
      assert.equal(s.events.length, 1);
      assert.equal(s.shots.length, opposing === "piercing" ? 2 : 1);
      s.dispose();
    });

test("a spent piercing shell uses normal interception and opposing damage ownership", () => {
  const s = arena(2),
    [aTank, bTank] = s.tanks;
  aTank.body.setTranslation({ x: 5, y: 0.65, z: -2.6 }, true);
  bTank.body.setTranslation({ x: 5, y: 0.65, z: 2.6 }, true);
  aTank.hp = bTank.hp = 40;
  s.world.step();
  const a = shot(s, "piercing", 0, 20, 0);
  a.owner = aTank.id;
  const first = shot(s, "rocket", 2, 0, 1);
  first.owner = bTank.id;
  const second = shot(s, "standard", 6, 0, 1);
  second.owner = bTank.id;
  s.shots = [second, first, a];
  stepProjectiles(s, 0.4);
  assert.equal(s.shots.length, 0);
  assert.equal(s.events.filter((e) => e.type === "explosion").length, 1);
  assert.equal(aTank.alive, false);
  assert.equal(bTank.alive, false);
  assert.equal(aTank.kills, 1);
  assert.equal(bTank.kills, 1);
  s.dispose();
});

test("piercing stops on tanks and cover and cannot intercept through thin cover", () => {
  const s = arena();
  s.human.body.setTranslation({ x: 2, y: 0.65, z: 0 }, true);
  s.world.step();
  const hp = s.human.hp;
  s.shots = [shot(s, "piercing", -2, 20, 1)];
  stepProjectiles(s, 0.3);
  assert.equal(s.shots.length, 0);
  assert.equal(s.human.hp, hp - 40);
  s.addCover({ kind: "wall", x: 0, z: 0, w: 0.1, d: 10, h: 3, hp: Infinity, color: 0 });
  s.world.step();
  s.events = [];
  s.shots = [shot(s, "piercing", -0.3, 20, 0), shot(s, "piercing", 0.3, -20, 1)];
  stepProjectiles(s, 0.05);
  assert.equal(s.shots.length, 0);
  assert.equal(s.events.filter((e) => e.type === "explosion" || e.type === "ricochet").length, 0);
  assert.ok(
    s.events.every((e) => e.size === 0.6),
    "only cover impacts, no shell interception",
  );
  s.dispose();
});

test("Pine Village has eight mirrored ammo crates with navigable approaches from both teams", () => {
  const s = new Simulation(123),
    ammo = s.pickups.filter((p) => isSpecialAmmo(p.kind));
  assert.equal(ammo.length, 8);
  for (const kind of AMMO_ORDER.filter(isSpecialAmmo))
    assert.equal(ammo.filter((p) => p.kind === kind).length, 2);
  for (const p of ammo) {
    assert.ok(ammo.some((q) => q.kind === p.kind && q.x === -p.x && q.z === -p.z));
    assert.ok(Math.hypot(p.x, p.z) > 20);
    assert.equal(s.nav.blocked[s.nav.index(p)], 0);
    for (const team of [0, 1] as const)
      for (const spawn of spawnPositions(team)) {
        const path = s.nav.find(spawn, p);
        assert.ok(path.length > 0 && distance(path.at(-1)!, p) < 1.8);
      }
  }
  s.dispose();
});

test("bot roles select stocked ammo, use standard on cover, and fall back after depletion", () => {
  const s = arena(2),
    [bot, enemy] = s.tanks;
  bot.human = false;
  enemy.human = true;
  bot.ammo = { spread: 5, rocket: 5, ricochet: 5, piercing: 5 };
  for (const role of Object.keys(BOT_AMMO) as (keyof typeof BOT_AMMO)[]) {
    bot.brain.personality = role;
    bot.brain.decision = 0;
    assert.equal(preferredAmmo(bot), BOT_AMMO[role]);
    assert.equal(botCommand(s, bot, STEP).ammoSelection, BOT_AMMO[role]);
  }
  enemy.body.setTranslation({ x: 0, y: 0.65, z: 50 }, true);
  bot.brain.target = 0;
  bot.brain.memory = 0;
  bot.brain.decision = 99;
  bot.brain.goal = { x: 0, z: 10 };
  s.addCover({ kind: "fence", x: 0, z: 7, w: 4, d: 0.6, h: 2, hp: 60, color: 0 });
  s.world.step();
  assert.equal(botCommand(s, bot, STEP).ammoSelection, "standard");
  bot.ammo = emptyAmmo();
  bot.ammo.rocket = 1;
  bot.selectedAmmo = "rocket";
  fireWeapon(s, bot);
  assert.equal(bot.selectedAmmo, "standard");
  bot.cooldown = 0;
  fireWeapon(s, bot);
  assert.equal(s.shots.at(-1)!.weapon, "standard");
  s.dispose();
});

test("bots drive to useful crates, consume ammo and ignore a full crate", () => {
  const s = arena(),
    bot = s.human;
  bot.human = false;
  bot.brain.personality = "artillery";
  const p = crate(s, "rocket");
  p.z = 6;
  s.pickups.push(p);
  bot.brain.decision = 0;
  botCommand(s, bot, STEP);
  assert.equal(bot.brain.mode, "pickup");
  for (let i = 0; i < 180 && p.available; i++) s.step();
  assert.equal(p.available, false);
  assert.equal(bot.ammo.rocket, 12);
  selectAmmo(bot, preferredAmmo(bot));
  fireWeapon(s, bot);
  assert.equal(bot.ammo.rocket, 11);
  bot.ammo.rocket = 24;
  p.available = true;
  bot.brain.decision = 0;
  botCommand(s, bot, STEP);
  assert.notEqual(bot.brain.mode, "pickup");
  s.dispose();
});
