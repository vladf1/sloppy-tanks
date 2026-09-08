import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { RANKS, rankIndex, earnExperience, repairVeteran } from "../src/game/veterancy";
import { fireWeapon, weaponInterval, placeMine, stepMines, stepProjectiles, collectPickup } from "../src/game/weapons";
import { botReload } from "../src/game/bot-personalities";
import { AMMO_ORDER, refillAmmo } from "../src/game/ammunition";
import { STEP, VEHICLES, WEAPONS } from "../src/game/data";
import { idleCommand, type Tank, type VehicleKind } from "../src/game/types";
before(async () => { await RAPIER.init(); });
const near = (a: number, b: number) => assert.ok(Math.abs(a - b) < 1e-8, `${a} != ${b}`);
function move(t: Tank, x: number, z: number) {
  t.body.setTranslation({ x, y: 0.65, z }, true); t.previous = { x, z };
  t.body.setLinvel({ x: 0, y: 0, z: 0 }, true);
}
function fixture() {
  const s = new Simulation(123), player = s.human;
  const enemy = s.tanks.find(t => t.team !== player.team)!;
  const ally = s.tanks.find(t => !t.human && t.team === player.team)!;
  for (const t of s.tanks) if (![player, enemy, ally].includes(t)) s.world.removeRigidBody(t.body);
  s.tanks = [player, enemy, ally];
  for (const c of s.covers) s.world.removeRigidBody(c.body);
  s.covers = []; s.coverByCollider.clear(); s.nav.rebuild([]); s.pickups = [];
  s.tanks.forEach((t, i) => { t.protection = 0; move(t, i * 20, 0); });
  s.world.step(); s.start(); s.events = [];
  return { s, player, enemy, ally };
}

test("hull damage earns shared XP; only the finisher receives the kill bonus, without overkill XP", () => {
  const { s, player, enemy, ally } = fixture();
  const hp = enemy.hp;
  s.damageTank(enemy, 30, player.id, player.team);
  s.damageTank(enemy, 10, ally.id, ally.team);
  assert.equal(player.xp, 30); assert.equal(ally.xp, 10);
  s.damageTank(enemy, 9999, ally.id, ally.team);
  assert.equal(ally.xp, hp - 30 + 50); assert.equal(ally.kills, 1);
  s.damageTank(enemy, 9999, player.id, player.team);
  assert.equal(player.xp, 30); s.dispose();
});

test("spawn protection, allies, self-damage, shield absorption and scenery give no XP", () => {
  const { s, player, enemy, ally } = fixture();
  enemy.protection = 1; s.damageTank(enemy, 50, player.id, player.team); enemy.protection = 0;
  s.damageTank(ally, 50, player.id, player.team);
  s.damageTank(player, 10, player.id, player.team);
  enemy.shield = 12; enemy.shieldPoints = 60;
  s.damageTank(enemy, 50, player.id, player.team);
  const c = s.addCover({ kind: "fence", x: 0, z: 20, w: 2, d: 1, h: 1, hp: 10, color: 0 });
  s.damageCover(c, 999, player.id, player.team);
  assert.equal(player.xp, 0);
  s.damageTank(enemy, 25, player.id, player.team);
  assert.equal(player.xp, 15); s.dispose();
});

test("player and bots promote at exact thresholds, preserve hull percentage, and cap at Heroic", () => {
  const { s, player, ally } = fixture();
  for (const t of [player, ally]) {
    t.hp = s.maxHealth(t) * 0.4;
    earnExperience(s, t, 199.5); assert.equal(rankIndex(t), 0);
    earnExperience(s, t, 0.5); assert.equal(rankIndex(t), 1);
    near(t.hp, s.maxHealth(t) * 0.4);
    earnExperience(s, t, 300); assert.equal(rankIndex(t), 2);
    earnExperience(s, t, 500); assert.equal(rankIndex(t), 3);
    earnExperience(s, t, 99999); assert.equal(t.xp, 1000);
    near(t.hp, s.maxHealth(t) * 0.4);
    assert.equal(s.events.filter(e => e.type === "promotion" && e.id === t.id).length, 3);
  }
  s.dispose();
});

test("one large XP award reaches the correct rank and emits one promotion", () => {
  const { s, player } = fixture();
  earnExperience(s, player, 500);
  assert.equal(rankIndex(player), 2);
  assert.equal(s.events.filter(e => e.type === "promotion").length, 1);
  assert.match(s.events[0].label!, /ELITE/); s.dispose();
});

test("all five weapons snapshot rank damage; human and bot reload bonuses stack with rapid fire", () => {
  const { s, player, ally } = fixture();
  for (const t of [player, ally]) for (const weapon of AMMO_ORDER) {
    t.xp = 0; t.selectedAmmo = weapon;
    if (weapon !== "standard") refillAmmo(t, weapon);
    const rookie = weaponInterval(t), botRookie = botReload(t, 0, weapon);
    t.xp = 1000; t.rapid = 12; t.cooldown = 0;
    near(weaponInterval(t), rookie / 1.2 / 2);
    near(botReload(t, 0, weapon), botRookie / 1.2 / 2);
    s.shots = []; fireWeapon(s, t);
    assert.equal(s.shots.length, weapon === "spread" ? 3 : 1);
    for (const shot of s.shots) {
      near(shot.damage, WEAPONS[weapon].damage * 1.3);
      assert.equal(shot.ownerLife, t.deaths);
    }
    t.xp = 0; near(s.shots[0].damage, WEAPONS[weapon].damage * 1.3);
    t.rapid = 0;
  }
  s.dispose();
});

test("promotion scales a pending cannon reload and the bot decision timer", () => {
  const { s, ally } = fixture(); ally.cooldown = 0.6; ally.brain.fireDelay = 2;
  earnExperience(s, ally, 200);
  near(ally.cooldown, 0.6 / 1.1); near(ally.brain.fireDelay, 2 / 1.1); s.dispose();
});

test("mines snapshot damage and destroyed owners or replacements cannot gain XP from old ordnance", () => {
  for (const respawn of [false, true]) {
    const { s, player, enemy } = fixture();
    player.xp = 1000; placeMine(s, player); const mine = s.mines[0];
    assert.equal(mine.damage, 130); assert.equal(mine.ownerLife, 0);
    s.damageTank(player, 9999, player.id, player.team);
    if (respawn) { s.respawn(player); move(player, -30, -30); }
    const xp = player.xp;
    move(enemy, mine.x, mine.z); enemy.hp = 1; mine.arm = 0; stepMines(s, STEP);
    assert.equal(enemy.alive, false); assert.equal(player.xp, xp);
    // A late shell from that same life also leaves the new tank's progress alone.
    const fresh = s.addTank(enemy.team, false, "balanced", 0); move(fresh, 0, 10); fresh.protection = 0;
    s.world.step();
    s.shots.push({ id: s.nextId++, owner: player.id, ownerLife: 0, team: player.team,
      x: 0, z: 5, vx: 0, vz: 40, damage: 30, life: 2, bounces: 0, piercing: 0, weapon: "standard" });
    for (let i = 0; i < 12 && s.shots.length; i++) stepProjectiles(s, STEP);
    assert.equal(fresh.hp, s.maxHealth(fresh) - 30); assert.equal(player.xp, xp);
    s.dispose();
  }
});

test("mine and drum chains retain the initiating tank's XP and life attribution", () => {
  for (const oldLife of [false, true]) {
    const { s, player, enemy } = fixture(); move(enemy, 0, 12); enemy.hp = 10;
    const drum = s.addCover({ kind: "drum", x: 0, z: 10, w: 1, d: 1, h: 1, hp: 10, color: 0 });
    s.mines.push({ id: s.nextId++, owner: enemy.id, team: enemy.team, x: 0, z: 11, arm: 0, life: 20 });
    if (oldLife) { s.damageTank(player, 9999, player.id, player.team); s.respawn(player); }
    s.damageCover(drum, 10, player.id, player.team, 0);
    assert.equal(enemy.alive, false); assert.equal(player.xp, oldLife ? 0 : 60);
    assert.equal(s.mines.length, 0); s.dispose();
  }
});

test("Elite and Heroic repair only after five quiet seconds; shield hits and firing delay repair", () => {
  const { s, player, enemy, ally } = fixture();
  for (const bot of [enemy, ally]) s.world.removeRigidBody(bot.body);
  s.tanks = [player];
  for (const rank of RANKS) {
    player.xp = rank.xp; player.hp = 30; player.lastCombat = 0; s.elapsed = 4.9;
    repairVeteran(s, player, 1); assert.equal(player.hp, 30);
    s.elapsed = 5; repairVeteran(s, player, 1);
    near(player.hp, 30 + s.maxHealth(player) * rank.repair);
  }
  player.hp = 30; player.shield = 10; player.shieldPoints = 100;
  s.damageTank(player, 20, -1, enemy.team); assert.equal(player.lastCombat, 5);
  s.elapsed = 9.9; repairVeteran(s, player, 1); assert.equal(player.hp, 30);
  s.elapsed = 11; player.cooldown = 0;
  s.step({ ...idleCommand(), fire: true }); assert.equal(player.hp, 30);
  const time = s.elapsed; s.match.phase = "paused";
  for (let i = 0; i < 400; i++) s.step();
  assert.equal(s.elapsed, time); assert.equal(player.hp, 30);
  s.start(); s.shots = [];
  for (let i = 0; i < 361; i++) s.step();
  assert.ok(player.hp > 32 && player.hp < 33);
  player.hp = s.maxHealth(player) - 0.01; repairVeteran(s, player, 1);
  assert.equal(player.hp, s.maxHealth(player));
  s.damageTank(player, 9999, player.id, player.team); repairVeteran(s, player, 10);
  assert.equal(player.hp, 0); s.dispose();
});

test("promoted max hull and repair pickups respect every chassis and Solo difficulty scaling", () => {
  const { s, player, enemy } = fixture();
  for (const solo of [false, true]) for (const t of [player, enemy]) {
    s.gameMode = solo ? "solo" : "team";
    for (const kind of ["scout", "balanced", "heavy"] as VehicleKind[]) {
      t.kind = kind; t.xp = 1000; t.hp = 1;
      const expected = Math.round(VEHICLES[kind].health * (solo && t === enemy ? 0.4 : 1) * 1.2 * 100) / 100;
      near(s.maxHealth(t), expected);
      collectPickup(s, t, { id: s.nextId++, kind: "repair", x: 0, z: 0, available: true, cooldown: 0 });
      near(t.hp, expected);
    }
  }
  s.dispose();
});

test("death stops XP, respawn resets rank before computing new chassis hull, and round reset clears all", () => {
  const { s, player, ally } = fixture();
  for (const t of [player, ally]) {
    earnExperience(s, t, 1000); s.damageTank(t, 9999, t.id, t.team);
    earnExperience(s, t, 30); assert.equal(t.xp, 1000);
    s.humanKind = "heavy"; s.respawn(t);
    assert.equal(t.xp, 0); assert.equal(t.hp, VEHICLES[t.kind].health);
  }
  earnExperience(s, player, 500);
  const snapshot = s.snapshot();
  assert.equal(snapshot.tanks[0].rank, 2); assert.equal(snapshot.tanks[0].maxHp, 161);
  snapshot.tanks[0].xp = 0; assert.equal(player.xp, 500);
  s.reset(); assert.ok(s.tanks.every(t => t.xp === 0 && rankIndex(t) === 0)); s.dispose();
});
