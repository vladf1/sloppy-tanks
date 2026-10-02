import assert from "node:assert/strict";
import { test } from "node:test";
import type { EngineEvent } from "../src/game/engine-api";
import { deathCause, effectsLabel, rankTitle } from "../src/game/hud-feedback";

const death: EngineEvent = {
  type: "death",
  x: 0,
  z: 0,
  id: 4,
  owner: 8,
  damageSource: { cause: "rocket", origin: { x: 1, z: 2 } },
  playerHit: false,
  own: true,
  damageAngle: null,
};
const scoreboard = [
  { id: 4, name: "Player" },
  { id: 8, name: "Bob <b>literal</b>" },
];

test("death feedback explains another player's weapon, self damage, and environmental damage", () => {
  assert.equal(deathCause(death, scoreboard), "Bob <b>literal</b> killed you with a rocket blast.");
  assert.equal(
    deathCause({ ...death, owner: death.id }, scoreboard),
    "You destroyed yourself with a rocket blast.",
  );
  assert.equal(
    deathCause(
      { ...death, owner: undefined, damageSource: { cause: "drum", origin: { x: 1, z: 2 } } },
      scoreboard,
    ),
    "You were destroyed by an exploding barrel.",
  );
});

test("missing damage details and departed killers still produce a useful death explanation", () => {
  assert.equal(deathCause(death, []), "You were destroyed by a rocket blast.");
  assert.equal(
    deathCause({ ...death, damageSource: undefined }, scoreboard),
    "Bob <b>literal</b> killed you with an unknown weapon.",
  );
});

const noEffects = {
  protection: 0,
  shield: 0,
  shieldPoints: 0,
  rapid: 0,
  speed: 0,
  laser: 0,
  selfRepair: false,
};

test("active effects show remaining seconds, shield strength, and self repair together", () => {
  assert.equal(
    effectsLabel({
      protection: 0.4,
      shield: 4.1,
      shieldPoints: 79.1,
      rapid: 3.2,
      speed: 2.3,
      laser: 1.4,
      selfRepair: true,
    }),
    "SPAWN SHIELD  ◇ SHIELD 80 HP · 5s  » RAPID 4s  ϟ BOOST 3s  ✧ LASER DEFENSE 2s  SELF-REPAIR",
  );
  assert.equal(effectsLabel({ ...noEffects, rapid: 0.01 }), "» RAPID 1s");
});

test("expired effects disappear even when shield points remain in a snapshot", () => {
  assert.equal(effectsLabel(noEffects), "");
  assert.equal(effectsLabel({ ...noEffects, shieldPoints: 80, rapid: -0.01 }), "");
});

test("rank help uses the engine's bonuses and repair delay", () => {
  const rank = {
    rank: 0,
    rankDamage: 1,
    rankFireRate: 1,
    rankHealth: 1,
    rankRepair: 0,
    repairDelay: 5,
  };
  assert.equal(
    rankTitle(rank),
    "Earn XP from enemy hull damage and kills. Ranks reset on respawn.",
  );
  assert.equal(
    rankTitle({
      ...rank,
      rank: 2,
      rankDamage: 1.2,
      rankFireRate: 1.15,
      rankHealth: 1.15,
      rankRepair: 0.01,
    }),
    "+20% damage · +15% fire rate · +15% hull · repairs 1% hull/s after 5s out of combat",
  );
});
