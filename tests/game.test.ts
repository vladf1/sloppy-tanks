import { test, before } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { newMatch, awardKill, tickMatch } from "../src/game/match";
import {
  collectPickup,
  fireWeapon,
  placeMine,
  stepMines,
  stepProjectiles,
} from "../src/game/weapons";
import { STEP, VEHICLES, Random } from "../src/game/data";
import { idleCommand, type Team } from "../src/game/types";
before(async () => {
  await RAPIER.init();
});
function game() {
  const s = new Simulation(123);
  s.start();
  for (const t of s.tanks) t.protection = 0;
  return s;
}
function place(s: Simulation, id: number, x: number, z: number) {
  const t = s.tanks[id];
  t.body.setTranslation({ x, y: 0.65, z }, true);
  t.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
  t.previous = { x, z };
  s.world.step();
  return t;
}
function clear(s: Simulation) {
  for (const c of s.covers)
    if (c.kind !== "boundary") {
      c.alive = false;
      s.world.removeRigidBody(c.body);
    }
  s.nav.rebuild(s.covers);
  for (let i = 0; i < s.tanks.length; i++) place(s, i, -30 + i * 3, 30);
}
test("few-hit combat, friendly safety, exactly-once kills and attribution", () => {
  const s = game(),
    a = s.tanks[0],
    b = s.tanks[1];
  s.damageTank(a, 40, s.tanks[2].id, 0);
  assert.equal(a.hp, VEHICLES[a.kind].health);
  s.damageTank(b, 1000, a.id, 0);
  assert.equal(s.match.scores[0], 1);
  s.damageTank(b, 1000, a.id, 0);
  assert.equal(s.match.scores[0], 1);
  assert.equal(a.kills, 1);
  s.dispose();
});
test("self explosion damages owner and self-kill awards no point", () => {
  const s = game(),
    a = s.tanks[0];
  s.explode(a.body.translation(), 6, 1000, a.id, a.team);
  assert.equal(a.alive, false);
  assert.deepEqual(s.match.scores, [0, 0]);
  s.dispose();
});
test("protection prevents damage, expires after 2 seconds, firing cancels it", () => {
  const s = game(),
    a = s.human;
  s.human.protection = 2;
  s.damageTank(a, 1000, 999, (1 - a.team) as Team);
  assert.ok(a.alive);
  fireWeapon(s, a);
  assert.equal(a.protection, 0);
  a.protection = 2;
  for (let i = 0; i < 121; i++) s.step();
  assert.equal(a.protection, 0);
  s.dispose();
});
test("respawn occurs after three seconds with protection and selected class", () => {
  const s = game(),
    a = s.human;
  s.damageTank(a, 1000, 999, (1 - a.team) as Team);
  s.humanKind = "heavy";
  for (let i = 0; i < 179; i++) s.step();
  assert.equal(a.alive, false);
  for (let i = 0; i < 3; i++) s.step();
  assert.ok(a.alive);
  assert.equal(a.hp, 140);
  assert.ok(a.protection > 1.9);
  s.dispose();
});
test("special weapons replace and expire; upgrades persist independently and repair fully heals", () => {
  const s = game(),
    a = s.human;
  const p = {
    id: 1,
    x: 0,
    z: 0,
    kind: "rapid" as const,
    available: true,
    cooldown: 0,
  };
  collectPickup(s, a, p);
  assert.equal(a.weapon, "standard");
  assert.equal(a.rapid, 12);
  collectPickup(s, a, { ...p, kind: "rocket", available: true });
  assert.equal(a.weapon, "rocket");
  a.weaponTime = STEP;
  s.step();
  assert.equal(a.weapon, "standard");
  a.shield = 7;
  collectPickup(s, a, { ...p, kind: "shield", available: true });
  assert.equal(a.shield, 15);
  assert.equal(a.shieldPoints, 120);
  a.hp = 1;
  collectPickup(s, a, { ...p, kind: "repair", available: true });
  assert.equal(a.hp, VEHICLES[a.kind].health);
  collectPickup(s, a, { ...p, kind: "repair", available: true });
  assert.equal(a.hp, VEHICLES[a.kind].health);
  s.dispose();
});
test("swept fast shell hits a target between frame endpoints and ignores ally", () => {
  const s = game();
  clear(s);
  const a = place(s, 0, -12, 0),
    ally = place(s, 2, -7, 0),
    enemy = place(s, 1, -2, 0);
  s.shots.push({
    id: 999,
    x: -12,
    z: 0,
    vx: 1200,
    vz: 0,
    owner: a.id,
    team: 0,
    damage: 40,
    bounces: 1,
    life: 2,
    weapon: "standard",
  });
  stepProjectiles(s, STEP);
  assert.equal(enemy.hp, VEHICLES[enemy.kind].health - 40);
  assert.equal(ally.hp, 100);
  assert.equal(s.shots.length, 0);
  s.dispose();
});
test("standard ricochet reflects once off surviving cover and removes on next hit", () => {
  const s = game();
  clear(s);
  s.addCover({
    kind: "concrete",
    x: 0,
    z: 0,
    w: 1,
    d: 10,
    h: 2,
    hp: Infinity,
    color: 0,
  });
  s.world.step();
  s.shots.push({
    id: 999,
    x: -3,
    z: 0,
    vx: 180,
    vz: 0,
    owner: s.human.id,
    team: s.humanTeam,
    damage: 40,
    bounces: 1,
    life: 2,
    weapon: "standard",
  });
  stepProjectiles(s, STEP);
  assert.equal(s.shots[0].bounces, 0);
  assert.ok(s.shots[0].vx < 0);
  s.dispose();
});
test("destroyed cover does not reflect shells and breaks exactly once", () => {
  const s = game();
  clear(s);
  const c = s.addCover({
    kind: "wall",
    x: 0,
    z: 0,
    w: 1,
    d: 10,
    h: 2,
    hp: 40,
    color: 0,
  });
  s.world.step();
  s.shots.push({
    id: 999,
    x: -3,
    z: 0,
    vx: 180,
    vz: 0,
    owner: s.human.id,
    team: s.humanTeam,
    damage: 40,
    bounces: 1,
    life: 2,
    weapon: "standard",
  });
  stepProjectiles(s, STEP);
  assert.equal(c.alive, false);
  assert.equal(s.shots.length, 0);
  s.damageCover(c, 100, 0, 0);
  assert.equal(s.destroyed, 1);
  s.dispose();
});
test("mines arm after delay, ignore allies, preserve original owner", () => {
  const s = game();
  clear(s);
  const a = place(s, 0, 0, 0),
    b = place(s, 1, 1, 0);
  b.hp = 40;
  placeMine(s, a);
  assert.equal(s.mines.length, 1);
  stepMines(s, 0.5);
  assert.ok(b.alive);
  assert.equal(s.mines.length, 1);
  stepMines(s, 0.4);
  assert.equal(s.mines.length, 0);
  assert.equal(b.alive, false);
  assert.equal(s.match.scores[0], 1);
  s.dispose();
});
test("drum chain kills keep the initiating team and self-kills do not score", () => {
  const s = game();
  clear(s);
  const a = place(s, 0, -20, 0),
    b = place(s, 1, 6, 0);
  b.hp = 20;
  const d1 = s.addCover({
      kind: "drum",
      x: 0,
      z: 0,
      w: 1,
      d: 1,
      h: 2,
      hp: 30,
      color: 0,
    }),
    d2 = s.addCover({
      kind: "drum",
      x: 4,
      z: 0,
      w: 1,
      d: 1,
      h: 2,
      hp: 30,
      color: 0,
    });
  s.damageCover(d1, 40, a.id, a.team);
  assert.equal(d2.alive, false);
  assert.equal(b.alive, false);
  assert.equal(a.kills, 1);
  assert.equal(s.destroyed, 2);
  s.dispose();
});
test("tower collapse opens center route and retains side rubble", () => {
  const s = game();
  const tower = s.covers.find((c) => c.kind === "tower")!;
  const before = s.nav.blocked[s.nav.index(tower)];
  const version = s.nav.version;
  s.damageCover(tower, 999, s.human.id, s.humanTeam);
  assert.equal(before, 1);
  assert.equal(s.nav.blocked[s.nav.index(tower)], 0);
  assert.ok(s.nav.version > version);
  assert.equal(s.covers.filter((c) => c.kind === "rubble").length, 2);
  const path = s.nav.find(
    { x: tower.x, z: tower.z - 6 },
    { x: tower.x, z: tower.z + 6 },
  );
  assert.ok(
    path.some(
      (p) => Math.abs(p.x - tower.x) < 1 && Math.abs(p.z - tower.z) < 2,
    ),
  );
  s.dispose();
});
test("match time, tie overtime, next valid kill and 50 kill limit", () => {
  const m = newMatch();
  m.phase = "playing";
  m.time = STEP;
  tickMatch(m, STEP);
  assert.ok(m.overtime);
  awardKill(m, 0, 0, true);
  assert.equal(m.phase, "playing");
  awardKill(m, 0, 1, false);
  assert.equal(m.winner, 1);
  const n = newMatch();
  n.phase = "playing";
  n.scores = [49, 48];
  awardKill(n, 1, 0, false);
  assert.equal(n.winner, 0);
  const timed = newMatch();
  timed.phase = "playing";
  timed.time = 0.1;
  timed.scores = [2, 3];
  tickMatch(timed, 0.2);
  assert.equal(timed.winner, 1);
});
test("complete reset restores counts, cover, pickups, scores, nav and RNG", () => {
  const s = game();
  const counts = s.snapshot().counts;
  for (const c of [...s.covers]) s.damageCover(c, 999, s.human.id, s.humanTeam);
  s.shotsFired = 100;
  s.match.scores = [20, 10];
  s.reset();
  assert.deepEqual(s.snapshot().counts, counts);
  assert.deepEqual(s.match.scores, [0, 0]);
  assert.equal(s.destroyed, 0);
  assert.ok(s.pickups.every((p) => p.available));
  s.dispose();
});
test("seeded random and team roster are reproducible and symmetric", () => {
  const a = new Random(9),
    b = new Random(9);
  assert.deepEqual(
    Array.from({ length: 20 }, () => a.next()),
    Array.from({ length: 20 }, () => b.next()),
  );
  const s = game();
  assert.equal(s.tanks.filter((t) => t.human).length, 1);
  assert.equal(s.tanks.filter((t) => t.team === 0).length, 6);
  assert.equal(s.tanks.filter((t) => t.team === 1).length, 6);
  s.dispose();
});
test("bots independently fight on both teams while human is idle", () => {
  const s = game();
  for (let i = 0; i < 60 * 45; i++) s.step(idleCommand());
  assert.ok(
    s.match.scores[0] > 0 && s.match.scores[1] > 0,
    JSON.stringify(s.match),
  );
  assert.equal(s.human.kills, 0);
  assert.ok(s.botReroutes > 0);
  s.dispose();
});
test("chain-triggered mines are removed safely during mine iteration", () => {
  const s = game();
  clear(s);
  const a = place(s, 0, -20, 0),
    b = place(s, 1, 1, 0);
  b.hp = 20;
  for (const x of [0, 1, 2, 3])
    s.mines.push({
      id: s.nextId++,
      x,
      z: 0,
      owner: a.id,
      team: a.team,
      arm: 0,
      life: 25,
    });
  stepMines(s, STEP);
  assert.equal(s.mines.length, 0);
  assert.equal(s.match.scores[0], 1);
  s.dispose();
});
test("ricochet core adds two reflections and doubles damage; standard has one", () => {
  const s = game();
  const t = s.human;
  t.ricochet = 12;
  fireWeapon(s, t);
  assert.equal(s.shots.at(-1)!.bounces, 3);
  assert.equal(s.shots.at(-1)!.damage, 80);
  t.cooldown = 0;
  t.ricochet = 0;
  t.weapon = "standard";
  fireWeapon(s, t);
  assert.equal(s.shots.at(-1)!.bounces, 1);
  s.dispose();
});
test("bots cross opened tower footprint and continue combat through ruins", () => {
  const s = game();
  const towers = s.covers.filter((c) => c.kind === "tower");
  for (const c of towers) s.damageCover(c, 999, s.human.id, s.humanTeam);
  // Place a scout at the entrance with a useful pickup beyond the shortcut.
  // This exercises steering through the opening without relying on a random patrol.
  const tower = towers[0];
  const scoutIndex = s.tanks.findIndex((t) => !t.human && t.kind === "scout");
  const scout = place(s, scoutIndex, tower.x, tower.z - 5);
  scout.brain.personality = "scout";
  s.pickups.push({
    id: s.nextId++,
    kind: "rapid",
    x: tower.x,
    z: tower.z + 5,
    available: true,
    cooldown: 0,
  });
  let crossed = false;
  for (let i = 0; i < 60 * 45; i++) {
    s.step(idleCommand(), true);
    for (const t of s.tanks)
      if (t.alive) {
        const p = t.body.translation();
        if (
          towers.some(
            (c) => Math.abs(p.x - c.x) < 1.2 && Math.abs(p.z - c.z) < 2,
          )
        )
          crossed = true;
      }
  }
  assert.ok(crossed, "a bot traverses an opened shortcut");
  assert.ok(s.match.scores[0] > 0 && s.match.scores[1] > 0);
  s.dispose();
});
test("fragment cap bounds bodies and cosmetics cannot block live tanks", () => {
  const s = game();
  const initial = s.world.bodies.len();
  for (let i = 0; i < 300; i++) s.fragment(0, 0, 0, 0.5);
  assert.equal(s.fragments.length, 80);
  assert.equal(s.world.bodies.len(), initial + 80);
  assert.equal(s.fragments[0].body.collider(0).collisionGroups() & 1, 0);
  s.dispose();
});
test("arena cover, pickup types and spawn slots have rotated team symmetry", async () => {
  const { arenaLayout, pickupLayout, spawnPositions } =
    await import("../src/game/arena");
  const covers = arenaLayout();
  for (const c of covers)
    assert.ok(
      covers.some(
        (o) =>
          o.kind === c.kind &&
          o.x === -c.x &&
          o.z === -c.z &&
          o.w === c.w &&
          o.d === c.d,
      ),
      JSON.stringify(c),
    );
  for (const p of pickupLayout)
    assert.ok(
      pickupLayout.some(
        (o) => o.kind === p.kind && o.x === -p.x && o.z === -p.z,
      ),
      JSON.stringify(p),
    );
  const a = spawnPositions(0),
    b = spawnPositions(1);
  for (let i = 0; i < a.length; i++) {
    assert.equal(a[i].x, -b[i].x);
    assert.equal(a[i].z, -b[i].z);
  }
});
test("deaths during a full fragment burst remain within the shared debris and wreck cap", () => {
  const s = game();
  for (let i = 0; i < 150; i++) s.fragment(0, 0, 0, 0.5);
  for (const t of s.tanks) {
    s.damageTank(t, 1000, 999, (1 - t.team) as Team);
    assert.ok(s.fragments.length <= 80);
  }
  s.dispose();
});

test("expanded flanks have navigable routes from both spawn lines", async () => {
  const { spawnPositions } = await import("../src/game/arena");
  const s = game();
  for (const team of [0, 1] as const)
    for (const start of spawnPositions(team))
      for (const z of [-53, 53]) {
        const path = s.nav.find(start, { x: 0, z });
        assert.ok(path.length > 0, JSON.stringify({ start, z }));
        const end = path.at(-1)!;
        assert.ok(Math.abs(end.x) < 1 && Math.abs(end.z - z) < 1);
      }
  s.dispose();
});
test("tank breakup varies assemblies, travels widely, lands, and clears after flight", () => {
  const variants = new Set<string>();
  let highest = 0;
  for (let seed = 1; seed <= 12; seed++) {
    const s = game();
    clear(s);
    const t = place(s, s.tanks.indexOf(s.human), 0, 0);
    s.rng = new Random(seed);
    s.damageTank(t, 1000, 999, (1 - t.team) as Team);
    const pieces = [...s.fragments];
    const names = pieces.map((f) => f.part).sort();
    assert.ok(names.includes("hull"));
    assert.ok(
      names.includes("turret-barrel") ||
        (names.includes("turret") && names.includes("barrel")),
    );
    variants.add(names.join("/"));
    assert.ok(pieces.length <= 3);
    for (let i = 0; i < 180; i++) {
      s.world.step();
      highest = Math.max(highest, ...pieces.map((f) => f.body.translation().y));
    }
    assert.ok(
      pieces.every((f) => f.body.translation().y < 2),
      "parts land before respawn camera moves",
    );
    const a = pieces[0].body.translation(),
      b = pieces[1].body.translation();
    assert.ok(
      Math.hypot(a.x - b.x, a.z - b.z) > 12,
      "hull and turret separate by several tank lengths",
    );
    for (const tank of s.tanks) tank.cooldown = 100;
    const ids = new Set(pieces.map((f) => f.id));
    for (let i = 0; i < 360; i++) s.step();
    assert.ok(s.fragments.every((f) => !ids.has(f.id)));
    s.dispose();
  }
  assert.equal(variants.size, 2);
  assert.ok(highest > 10, "some turrets take high arcs");
});

test("bots pause between shots even when breaching; human fires faster with and without rapid upgrade", async () => {
  const { botCommand } = await import("../src/game/ai");
  const { WEAPONS } = await import("../src/game/data");
  for (const rapid of [false, true]) {
    const s = game();
    clear(s);
    const bot = place(
      s,
      s.tanks.findIndex((t) => !t.human),
      -8,
      0,
    );
    bot.aim = Math.PI / 2;
    bot.rapid = rapid ? 12 : 0;
    bot.brain.goal = { x: 4, z: 0 };
    bot.brain.decision = bot.brain.memory = 100;
    s.addCover({
      kind: "wall",
      x: 0,
      z: 0,
      w: 1,
      d: 5,
      h: 2,
      hp: 10000,
      color: 0,
    });
    const human = s.human;
    human.rapid = rapid ? 12 : 0;
    let botShots = 0,
      humanShots = 0;
    for (let i = 0; i < 600; i++) {
      bot.cooldown = Math.max(0, bot.cooldown - STEP);
      human.cooldown = Math.max(0, human.cooldown - STEP);
      const command = botCommand(s, bot, STEP);
      if (command.fire && bot.cooldown === 0) {
        fireWeapon(s, bot);
        botShots++;
      }
      if (human.cooldown === 0) {
        fireWeapon(s, human);
        humanShots++;
      }
    }
    assert.ok(
      botShots > 0 && botShots < humanShots * 0.8,
      JSON.stringify({ rapid, botShots, humanShots }),
    );
    assert.ok(humanShots >= Math.floor(10 / (WEAPONS.standard.interval * (rapid ? 0.5 : 1) / 1.2 + STEP)));
    s.dispose();
  }
});

test("village buildings and trees block routes until destroyed, while all spawns reach midfield", async () => {
  const { spawnPositions } = await import("../src/game/arena");
  const s = game();
  for (const kind of ["house", "tree", "fence"] as const) {
    const c = s.covers.find((c) => c.kind === kind && c.destructible)!;
    assert.ok(c && c.destructible);
    assert.equal(s.nav.blocked[s.nav.index(c)], 1);
    s.damageCover(c, 1000, s.human.id, s.humanTeam);
    assert.equal(c.alive, false);
    assert.equal(s.nav.blocked[s.nav.index(c)], 0);
  }
  for (const team of [0, 1] as const)
    for (const p of spawnPositions(team))
      assert.ok(s.nav.find(p, { x: 0, z: 0 }).length > 0);
  s.dispose();
});
