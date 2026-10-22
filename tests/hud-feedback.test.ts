import assert from "node:assert/strict";
import { test } from "node:test";
import type { EngineEvent } from "../src/game/engine-api";
import {
  activeEffects,
  deathCause,
  deathReport,
  effectsLabel,
  feedIcon,
  hullTone,
  isOwnKill,
  killFeedNames,
  rankTitle,
} from "../src/game/hud-feedback";
import { teamOutcome } from "../src/game/ui-markup";

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
    "Bob <b>literal</b> destroyed you.",
  );
  assert.equal(
    deathCause({ ...death, owner: death.id, damageSource: undefined }, scoreboard),
    "You destroyed yourself.",
  );
  assert.equal(
    deathCause({ ...death, owner: undefined, damageSource: undefined }, scoreboard),
    "You were destroyed.",
  );
});

test("the respawn card names the killer and weapon apart, and nobody for self damage", () => {
  assert.deepEqual(deathReport(death, scoreboard), {
    killer: "Bob <b>literal</b>",
    self: false,
    cause: "rocket",
    weapon: "Rocket blast",
  });
  assert.deepEqual(deathReport({ ...death, owner: 4, damageSource: undefined }, scoreboard), {
    killer: null,
    self: true,
    cause: null,
    weapon: null,
  });
});

test("kill feed names the viewer as YOU and an ownerless kill as the yard", () => {
  assert.deepEqual(killFeedNames(death, 4, scoreboard), ["Bob <b>literal</b>", "YOU"]);
  assert.deepEqual(killFeedNames({ id: 8, owner: undefined }, 4, scoreboard), [
    "YARD",
    "Bob <b>literal</b>",
  ]);
});

test("an own kill follows tank identity, not a player named YOU", () => {
  assert.equal(isOwnKill({ id: 7, owner: 4 }, 4), true);
  assert.equal(isOwnKill({ id: 4, owner: 4 }, 4), false);
  assert.equal(isOwnKill({ id: 4, owner: 7 }, 4), false);
  assert.equal(isOwnKill({ id: 7, owner: undefined }, 4), false);
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

test("the hull bar's colour follows the hull's state, never the team", () => {
  assert.deepEqual([1, 0.61, 0.6, 0.31, 0.3, 0].map(hullTone), [
    "healthy",
    "healthy",
    "damaged",
    "damaged",
    "critical",
    "critical",
  ]);
});

test("every kill-feed row carries an icon, the wreck mark when the weapon is unknown", () => {
  assert.equal(feedIcon("rocket"), "hud-rocket");
  assert.equal(feedIcon("tow"), "hud-rocket");
  assert.equal(feedIcon(undefined), "hud-skull");
});

test("effect chips name each effect apart from what is left of it", () => {
  assert.deepEqual(activeEffects({ ...noEffects, shield: 4.1, shieldPoints: 79.1, rapid: 3.2 }), [
    { kind: "shield", name: "SHIELD", detail: "80 HP · 5s" },
    { kind: "rapid", name: "RAPID", detail: "4s" },
  ]);
  assert.deepEqual(activeEffects(noEffects), []);
});

test("the battle report names a win, a loss or a draw, even for a battle ended early", () => {
  assert.equal(teamOutcome([100, 80], 0, 0, false), "won");
  assert.equal(teamOutcome([100, 80], 1, 0, false), "lost");
  // Ended early: the side ahead when it ended takes it; level scores are a draw.
  assert.equal(teamOutcome([3, 4], 1, null, true), "won");
  assert.equal(teamOutcome([3, 4], 0, null, true), "lost");
  assert.equal(teamOutcome([0, 0], 0, null, true), "draw");
});
