import { endBattle } from "../src/game/match";
import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { combatFeats, recapStats, savePersonalBests } from "../src/game/round-recap";
import { earnExperience } from "../src/game/veterancy";
import { longestLife, recordKill } from "../src/game/combat-record";
import { idleCommand } from "../src/game/types";
import { fireWeapon, stepProjectiles, collectPickup } from "../src/game/weapons";
import { respawnTank } from "../src/game/tank-lifecycle";

before(async () => {
  await RAPIER.init();
});
test("recap counts actual enemy hull damage, preserves peaks and resets with the round", () => {
  const s = new Simulation(123);
  try {
    s.start();
    const player = s.human;
    const enemy = s.tanks.find((t) => t.team !== player.team)!;
    const ally = s.tanks.find((t) => !t.human && t.team === player.team)!;
    enemy.protection = ally.protection = player.protection = 0;
    s.damageTank(ally, 20, player.id, player.team, 0);
    s.damageTank(player, 10, player.id, player.team, 0);
    assert.equal(player.damageDealt, 0);
    enemy.shield = 10;
    enemy.shieldPoints = 30;
    s.damageTank(enemy, 40, player.id, player.team, 0);
    assert.equal(player.damageDealt, 10);
    const remaining = enemy.hp;
    s.damageTank(enemy, 999, player.id, player.team, 0);
    assert.equal(player.damageDealt, 10 + remaining);
    assert.equal(player.bestLifeKills, 1);
    earnExperience(s, player, 1500, 0);
    assert.equal(player.highestRank, 3);
    s.damageTank(player, 9999, enemy.id, enemy.team, enemy.deaths);
    respawnTank(s, player);
    assert.equal(player.lifeKills, 0);
    assert.equal(player.bestLifeKills, 1);
    assert.equal(player.highestRank, 3);
    assert.equal(player.xp, 0);
    respawnTank(s, enemy);
    enemy.protection = 0;
    s.damageTank(enemy, 9999, player.id, player.team, 0);
    assert.equal(player.kills, 2);
    assert.equal(player.lifeKills, 0, "old-life ordnance cannot pad the new life");
    s.reset();
    assert.equal(s.human.damageDealt, 0);
    assert.equal(s.human.highestRank, 0);
    assert.equal(s.human.bestLifeKills, 0);
  } finally {
    s.dispose();
  }
});

test("records distinguish a first round, improvements, ties and separate categories", () => {
  const values = new Map<string, string>();
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
  };
  const first = {
    kills: 5,
    damage: 450,
    bestLife: 3,
    rank: 1,
    busiestMinute: 3,
    longestLife: 90,
    multikill: 2,
    clutchKills: 0,
    revengeKills: 0,
    posthumousKills: 0,
    mineKills: 0,
    coverDestroyed: 0,
    pickups: 0,
  };
  assert.equal(savePersonalBests(storage, "team", first).established, false);
  const next = savePersonalBests(storage, "team", {
    ...first,
    kills: 6,
    damage: 400,
    bestLife: 3,
    rank: 2,
  });
  assert.deepEqual([...next.improved], ["kills", "rank"]);
  assert.equal(next.best.damage, 450);
  assert.equal(savePersonalBests(storage, "solo", first).established, false);
  values.set("bad", '{"kills": "oops", "rank": -1}');
  assert.equal(savePersonalBests(storage, "bad", first).established, false);
  const unavailable = {
    getItem: () => {
      throw Error();
    },
    setItem: () => {
      throw Error();
    },
  };
  assert.equal(savePersonalBests(unavailable, "team", first).persisted, false);
});

test("rolling kill windows cross clock-minute boundaries and expire exactly", () => {
  const s = new Simulation(123);
  try {
    const victim = s.tanks.find((t) => t.team !== s.human.team)!;
    for (const time of [58, 59, 61]) {
      s.elapsed = time;
      recordKill(s, s.human, victim, 0);
    }
    assert.equal(s.combatRecord.busiestMinute, 3);
    assert.equal(s.combatRecord.multikill, 3);
    s.elapsed = 121;
    recordKill(s, s.human, victim, 0);
    assert.deepEqual(s.combatRecord.recentKills, [121]);
    assert.equal(s.combatRecord.busiestMinute, 3);
    s.reset();
    assert.equal(s.combatRecord.busiestMinute, 0);
    s.elapsed = 1;
    recordKill(s, s.human, victim, 0);
    s.elapsed = 6;
    recordKill(s, s.human, victim, 0);
    assert.equal(s.combatRecord.multikill, 1, "five-second boundary is exclusive");
  } finally {
    s.dispose();
  }
});

test("longest life freezes on death, excludes respawn and paused time, includes unfinished life", () => {
  const s = new Simulation(123);
  try {
    const player = s.human;
    const enemy = s.tanks.find((t) => t.team !== player.team)!;
    s.start();
    s.elapsed = 42;
    player.protection = 0;
    s.damageTank(player, 9999, enemy.id, enemy.team);
    assert.equal(longestLife(s), 42);
    s.elapsed = 45;
    respawnTank(s, player);
    s.elapsed = 65;
    assert.equal(longestLife(s), 42);
    s.match.phase = "paused";
    for (let i = 0; i < 600; i++) {
      s.step(idleCommand());
    }
    assert.equal(longestLife(s), 42);
    s.elapsed = 100;
    assert.equal(longestLife(s), 55);
  } finally {
    s.dispose();
  }
});

test("revenge, clutch, posthumous and mine feats use credited kills", () => {
  const s = new Simulation(123);
  try {
    s.start();
    const p = s.human;
    const enemy = s.tanks.find((t) => t.team !== p.team)!;
    p.protection = 0;
    s.damageTank(p, 9999, enemy.id, enemy.team);
    respawnTank(s, p);
    p.hp = 1;
    enemy.protection = 0;
    s.damageTank(enemy, 9999, p.id, p.team, p.deaths, { cause: "mine", origin: { x: 0, z: 0 } });
    assert.equal(s.combatRecord.revengeKills, 1);
    assert.equal(s.combatRecord.clutchKills, 1);
    assert.equal(s.combatRecord.mineKills, 1);
    respawnTank(s, enemy);
    enemy.protection = 0;
    s.damageTank(enemy, 9999, p.id, p.team, p.deaths - 1);
    assert.equal(s.combatRecord.revengeKills, 1, "revenge is redeemed only once");
    assert.equal(s.combatRecord.clutchKills, 1, "old-life shell cannot earn a clutch kill");
    assert.equal(s.combatRecord.posthumousKills, 1);
    assert.ok(combatFeats(recapStats(s), 0, 0).some((f) => f.title === "DEAD BUT DANGEROUS"));
    assert.equal(
      combatFeats({ ...recapStats(s), posthumousKills: 0, revengeKills: 0 }, 0, 0).length,
      0,
    );
  } finally {
    s.dispose();
  }
});

test("combat counters measure shield/hull loss, actual pickups and attributable demolition", () => {
  const s = new Simulation(123);
  try {
    s.start();
    const p = s.human;
    const enemy = s.tanks.find((t) => t.team !== p.team)!;
    p.protection = 0;
    p.shield = 10;
    p.shieldPoints = 30;
    s.damageTank(p, 40, p.id, p.team);
    assert.equal(s.combatRecord.shieldAbsorbed, 30);
    assert.equal(s.combatRecord.damageTaken, 10);
    const pickup = s.pickups[0];
    pickup.kind = "repair";
    pickup.available = true;
    assert.equal(collectPickup(s, p, pickup), true);
    assert.equal(collectPickup(s, p, pickup), false);
    assert.equal(s.combatRecord.pickups, 1);
    const covers = s.covers.filter((c) => c.destructible && c.kind !== "drum");
    s.damageCover(covers[0], 9999, p.id, p.team);
    s.damageCover(covers[0], 9999, p.id, p.team);
    s.damageCover(covers[1], 9999, enemy.id, enemy.team);
    assert.equal(s.combatRecord.coverDestroyed, 1);
  } finally {
    s.dispose();
  }
});

test("direct hit rate counts emitted projectiles and enemy contacts, excluding protected hits", () => {
  const s = new Simulation(123);
  try {
    s.start();
    const p = s.human;
    const enemy = s.tanks.find((t) => t.team !== p.team)!;
    for (const cover of s.covers) {
      s.world.removeRigidBody(cover.body);
    }
    s.covers = [];
    s.movableCovers = [];
    s.coverByCollider.clear();
    for (const tank of s.tanks) {
      tank.body.setTranslation(
        { x: tank === p || tank === enemy ? 0 : 35, y: 0.65, z: tank === enemy ? 8 : 0 },
        true,
      );
    }
    p.aim = 0;
    enemy.protection = 0;
    s.world.step();
    fireWeapon(s, p);
    fireWeapon(s, p);
    assert.equal(s.combatRecord.shots, 1, "rejected cooldown fire isn't a shot");
    for (let i = 0; i < 60; i++) {
      stepProjectiles(s, 1 / 60);
    }
    assert.equal(s.combatRecord.directHits, 1);
    p.cooldown = 0;
    enemy.protection = 10;
    fireWeapon(s, p);
    for (let i = 0; i < 60; i++) {
      stepProjectiles(s, 1 / 60);
    }
    assert.equal(s.combatRecord.shots, 2);
    assert.equal(s.combatRecord.directHits, 1);
  } finally {
    s.dispose();
  }
});

test("ending a paused battle preserves the round and stats without claiming victory", () => {
  const s = new Simulation(123);
  try {
    endBattle(s.match);
    assert.equal(s.match.phase, "ready");
    s.start();
    s.elapsed = 42;
    s.human.kills = 3;
    s.match.scores = [7, 4];
    s.match.phase = "paused";
    const time = s.match.time;
    endBattle(s.match);
    assert.equal(s.match.phase, "results");
    assert.equal(s.match.endedEarly, true);
    assert.equal(s.match.winner, null);
    assert.equal(s.match.time, time);
    assert.deepEqual(s.match.scores, [7, 4]);
    assert.equal(recapStats(s).kills, 3);
    assert.equal(recapStats(s).longestLife, 42);
    s.step();
    assert.equal(s.elapsed, 42);
    s.reset();
    assert.equal(s.match.endedEarly, undefined);
  } finally {
    s.dispose();
  }
});
