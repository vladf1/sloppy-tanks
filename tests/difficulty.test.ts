import { before, test } from "node:test";
import assert from "node:assert/strict";
import RAPIER from "@dimforge/rapier3d-compat";
import { Simulation } from "../src/game/simulation";
import { DIFFICULTIES, parseDifficulty, type Difficulty } from "../src/game/difficulty";
import { SOLO } from "../src/game/simulation-rules";
import { botCommand } from "../src/game/ai";
import { clearArena } from "./fixtures";

before(async () => {
  await RAPIER.init();
});

/** The human, one enemy and one ally, 14 m apart along +z. */
function squad() {
  const s = new Simulation(123);
  const player = s.human;
  const enemy = s.tanks.find((t) => t.team !== player.team)!;
  const ally = s.tanks.find((t) => !t.human && t.team === player.team)!;
  clearArena(s, [player, enemy, ally]);
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
    const { s, player, enemy, ally } = squad();
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
      s.dispose();
    }
  });
}

test("difficulty applies to Solo damage before shield absorption", () => {
  const { s, player, enemy } = squad();
  try {
    s.gameMode = "solo";
    s.difficulty = "easy";
    player.shield = 10;
    player.shieldPoints = 3;
    const hp = player.hp;
    s.damageTank(player, 40, enemy.id, enemy.team);
    assert.equal(player.hp, hp - (40 * 0.9 * SOLO.enemyDamageMultiplier - 3));
    assert.equal(player.shieldPoints, 0);
  } finally {
    s.dispose();
  }
});

test("enemy aim, reaction and cadence vary monotonically; allies are unchanged", () => {
  function decision(level: Difficulty, friendly: boolean) {
    const { s, enemy, ally } = squad();
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
      s.dispose();
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
