import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { idleCommand, type Shot, type Weapon } from "../src/game/types";
import { DIFFICULTIES, parseDifficulty, type Difficulty } from "../src/game/difficulty";
import { SOLO } from "../src/game/simulation-rules";
import { botCommand } from "../src/game/ai";
import { fireWeapon, stepProjectiles } from "../src/game/weapons";
import { MINE } from "../src/game/combat-rules";
import { stepMines } from "../src/game/mines";

before(async () => {
  await RAPIER.init();
});
function arena() {
  const s = new Simulation(123);
  for (const cover of s.covers) s.world.removeRigidBody(cover.body);
  s.covers = [];
  s.coverByCollider.clear();
  s.pickups = [];
  s.nav.rebuild([]);
  const player = s.human;
  const enemy = s.tanks.find((t) => t.team !== player.team)!;
  const ally = s.tanks.find((t) => !t.human && t.team === player.team)!;
  for (const tank of s.tanks) {
    if (![player, enemy, ally].includes(tank)) s.world.removeRigidBody(tank.body);
  }
  s.tanks = [player, enemy, ally];
  for (const [i, tank] of s.tanks.entries()) {
    tank.protection = 0;
    tank.body.setTranslation({ x: 0, y: 0.65, z: i * 14 }, true);
    tank.previous = { x: 0, z: i * 14 };
  }
  s.world.step();
  s.start();
  return { s, player, enemy, ally };
}

for (const level of Object.keys(DIFFICULTIES) as Difficulty[]) {
  test(`${level}: enemy damage scales, allied and self damage retain baseline, reset keeps choice`, () => {
    const { s, player, enemy, ally } = arena();
    try {
      s.difficulty = level;
      const hp = player.hp;
      s.damageTank(player, 20, enemy.id, enemy.team);
      assert.equal(player.hp, hp - 20 * DIFFICULTIES[level].damage);
      const enemyHp = enemy.hp;
      s.damageTank(enemy, 20, ally.id, ally.team);
      assert.equal(enemy.hp, enemyHp - 20);
      const ownHp = player.hp;
      s.damageTank(player, 10, player.id, player.team);
      assert.equal(player.hp, ownHp - 10);
      const safeHp = player.hp;
      s.damageTank(player, 20, ally.id, ally.team);
      assert.equal(player.hp, safeHp);
      s.reset();
      assert.equal(s.difficulty, level);
    } finally {
      s.world.free();
    }
  });
}

test("difficulty applies to Solo damage before shield absorption", () => {
  const { s, player, enemy } = arena();
  try {
    s.gameMode = "solo";
    s.difficulty = "easy";
    player.shield = 10;
    player.shieldPoints = 3;
    const hp = player.hp;
    s.damageTank(player, 40, enemy.id, enemy.team);
    assert.equal(player.hp, hp - (40 * 0.7 * SOLO.enemyDamageMultiplier - 3));
    assert.equal(player.shieldPoints, 0);
  } finally {
    s.world.free();
  }
});

test("enemy aim, reaction and cadence vary monotonically; allies are unchanged", () => {
  function decision(level: Difficulty, friendly: boolean) {
    const { s, enemy, ally } = arena();
    try {
      s.difficulty = level;
      const tank = friendly ? ally : enemy;
      tank.brain.decision = 0;
      tank.brain.target = 0;
      botCommand(s, tank, 1 / 60);
      const reaction = tank.brain.reaction;
      const error = tank.brain.aimError;
      tank.brain.reaction = 0;
      tank.brain.decision = 10;
      tank.brain.fireDelay = 0;
      tank.aim = friendly ? Math.PI : 0;
      // Let the turret align, then capture a real firing decision.
      for (let i = 0; i < 240 && tank.brain.fireDelay === 0; i++) {
        tank.aim = botCommand(s, tank, 1 / 60).aim;
      }
      assert.ok(tank.brain.fireDelay > 0);
      return { reaction, error: Math.abs(error), reload: tank.brain.fireDelay };
    } finally {
      s.world.free();
    }
  }
  const easy = decision("easy", false),
    normal = decision("normal", false),
    hard = decision("hard", false);
  for (const key of ["reaction", "error", "reload"] as const) {
    assert.ok(easy[key] > normal[key], `Easy ${key}`);
    assert.ok(normal[key] > hard[key], `Hard ${key}`);
  }
  assert.deepEqual(decision("easy", true), decision("normal", true));
  assert.deepEqual(decision("hard", true), decision("normal", true));
  assert.equal(parseDifficulty("corrupt"), "normal");
  assert.equal(parseDifficulty(null), "normal");
});

test("empty selection emits feedback without switching, final special shot announces fallback once", () => {
  const { s, player } = arena();
  try {
    player.ammo.rocket = 1;
    player.selectedAmmo = "rocket";
    s.step({ ...idleCommand(), ammoSelection: "piercing" });
    assert.equal(player.selectedAmmo, "rocket");
    assert.match(s.events.find((e) => e.type === "notice")!.label!, /PIERCING EMPTY/);
    s.events = [];
    fireWeapon(s, player);
    assert.equal(player.selectedAmmo, "standard");
    assert.match(s.events.find((e) => e.type === "notice")!.label!, /switched to STANDARD/);
    fireWeapon(s, player);
    assert.equal(s.events.filter((e) => e.type === "notice").length, 1);
    s.events = [];
    s.match.phase = "paused";
    s.step({ ...idleCommand(), ammoSelection: "piercing" });
    assert.equal(s.events.length, 0);
  } finally {
    s.world.free();
  }
});

function incoming(s: Simulation, weapon: Weapon, owner: number, team: 0 | 1): Shot {
  return {
    id: s.nextId++,
    x: -5,
    z: 0,
    vx: 30,
    vz: 0,
    weapon,
    owner,
    team,
    damage: 40,
    bounces: 0,
    life: 3,
    piercing: weapon === "piercing" ? 1 : 0,
  };
}
for (const weapon of ["standard", "spread", "rocket", "ricochet", "piercing"] as const) {
  test(`${weapon} hit records actual impact direction and death cause`, () => {
    const { s, player, enemy } = arena();
    try {
      player.hp = 1;
      s.shots = [incoming(s, weapon, enemy.id, enemy.team)];
      stepProjectiles(s, 0.3);
      const death = s.events.find((e) => e.type === "death" && e.id === player.id)!;
      assert.ok(death);
      assert.equal(death.damageSource?.cause, weapon);
      assert.ok(
        death.damageSource!.origin.x < death.x,
        "hit from left despite attacker standing below",
      );
      assert.equal(death.owner, enemy.id);
    } finally {
      s.world.free();
    }
  });
}

test("mines, barrels and shell collisions preserve distinct death causes", () => {
  for (const cause of ["mine", "drum", "interception"] as const) {
    const { s, player, enemy, ally } = arena();
    try {
      player.hp = 1;
      if (cause === "mine") {
        s.mines.push({
          id: s.nextId++,
          x: 1,
          z: 0,
          owner: enemy.id,
          team: enemy.team,
          arm: 0,
          life: 10,
          damage: MINE.damage,
        });
        stepMines(s, 1 / 60);
      } else if (cause === "drum") {
        const drum = s.addCover({ kind: "drum", x: 2, z: 0, w: 1, d: 1, h: 2, hp: 1, color: 0 });
        s.damageCover(drum, 2, enemy.id, enemy.team);
      } else {
        s.shots = [
          { ...incoming(s, "standard", enemy.id, enemy.team), x: -1, z: 2.8, vx: 10 },
          { ...incoming(s, "standard", ally.id, ally.team), x: 1, z: 2.8, vx: -10 },
        ];
        stepProjectiles(s, 0.2);
      }
      const death = s.events.find((e) => e.type === "death" && e.id === player.id)!;
      assert.ok(death, cause);
      assert.equal(death.damageSource?.cause, cause);
      assert.equal(death.owner, enemy.id);
    } finally {
      s.world.free();
    }
  }
});

test("protected and fully shielded hits do not emit hull damage direction", () => {
  const { s, player, enemy } = arena();
  try {
    player.protection = 2;
    s.damageTank(player, 40, enemy.id, enemy.team, undefined, {
      cause: "standard",
      origin: { x: -1, z: 0 },
    });
    assert.equal(s.events.length, 0);
    player.protection = 0;
    player.shield = 10;
    player.shieldPoints = 100;
    s.damageTank(player, 40, enemy.id, enemy.team, undefined, {
      cause: "standard",
      origin: { x: -1, z: 0 },
    });
    assert.equal(s.events.length, 0);
    assert.equal(player.shieldPoints, 60);
  } finally {
    s.world.free();
  }
});

test("a reflected shell points toward its bounce, not the original shooter", () => {
  const { s, player, enemy } = arena();
  try {
    s.addCover({ kind: "wall", x: 5, z: 0, w: 1, d: 10, h: 3, hp: Infinity, color: 0 });
    s.world.step();
    s.shots = [{ ...incoming(s, "ricochet", enemy.id, enemy.team), x: 3, bounces: 1 }];
    stepProjectiles(s, 0.4);
    const hit = s.events.find((e) => e.type === "hurt" && e.id === player.id)!;
    assert.ok(hit);
    assert.equal(hit.damageSource?.cause, "ricochet");
    assert.ok(hit.damageSource!.origin.x > hit.x);
  } finally {
    s.world.free();
  }
});
