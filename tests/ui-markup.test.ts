import assert from "node:assert/strict";
import { test } from "node:test";
import type { HudState } from "../src/game/engine-api";
import { menuMarkup, ordinal, scoreMarkup } from "../src/game/ui-markup";

test("ordinals read as places: 1st, 2nd, 11th, 21st", () => {
  assert.deepEqual([1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111].map(ordinal), [
    "1ST",
    "2ND",
    "3RD",
    "4TH",
    "11TH",
    "12TH",
    "13TH",
    "21ST",
    "22ND",
    "101ST",
    "111TH",
  ]);
});

/** A team battle's HUD state, three kills to four, as the menus read it. */
function teamBattle(overrides: { phase: string; endedEarly?: boolean; alive?: boolean }): HudState {
  const human = {
    id: 1,
    kind: "balanced",
    vehicleName: "BRUISER",
    rankName: "Rookie",
    alive: overrides.alive ?? true,
    kills: 2,
    deaths: 1,
  };
  return {
    match: {
      phase: overrides.phase,
      scores: [3, 4],
      time: 284,
      overtime: false,
      winner: overrides.endedEarly ? null : 1,
      endedEarly: overrides.endedEarly ?? false,
      round: 1,
    },
    elapsed: 16,
    gameMode: "team",
    endlessMatch: false,
    mapName: "Pine Village",
    difficulty: "normal",
    humanTeam: 1,
    scoreLimit: 100,
    teamNames: ["BLUE", "RED"],
    activeEnemies: 6,
    human,
    scoreboard: [
      { id: 1, name: "YOU", kills: 2, deaths: 1, team: 1, kind: "balanced", human: true },
    ],
    recap: null,
  } as unknown as HudState;
}

test("pause leaves the score to the header's plates and lists the battle's facts", () => {
  const markup = menuMarkup(teamBattle({ phase: "paused" }), null);
  assert.match(markup, /class="dialog-score"/);
  assert.doesNotMatch(markup, /KILL SHARE|share-bar/);
  assert.match(markup, /<dt>Objective<\/dt><dd>First to 100<\/dd>/);
  assert.match(markup, /<dt>Kills \/ deaths<\/dt><dd>2 \/ 1<\/dd>/);
  assert.match(markup, /<dt>Difficulty<\/dt><dd>Normal<\/dd>/);
  // The line under the title adds what the plates do not say: the kills still to win.
  assert.match(
    markup,
    /class="dialog-standing" data-tone="ahead">Your team leads by 1 · 96 kills to win</,
  );
});

test("a battle ended early is called ended, never won, and says who was ahead", () => {
  const early = menuMarkup(teamBattle({ phase: "results", endedEarly: true }), null);
  assert.match(early, /<h2 id="results-title">BATTLE ENDED<\/h2>/);
  assert.match(early, /data-outcome="ended"/);
  assert.doesNotMatch(early, /VICTORY|ENDED BY YOU/);
  assert.match(early, /Your team led by 1 when you ended it · 1 death</);
  const full = menuMarkup(teamBattle({ phase: "results" }), null);
  assert.doesNotMatch(full, /ENDED BY YOU/);
  assert.match(full, /Your team took the round by 1 · 1 death</);
});

test("the respawn card names who destroyed the tank in plain words", () => {
  const markup = menuMarkup(teamBattle({ phase: "playing", alive: false }), {
    report: { killer: "THUNDERCLAP", self: false, cause: "standard", weapon: "Standard shell" },
    text: "THUNDERCLAP destroyed you with a standard shell.",
  });
  assert.match(markup, /<small>DESTROYED BY<\/small><b>THUNDERCLAP<\/b>/);
  assert.doesNotMatch(markup, /WRECKED/);
});

test("dialog score plates mark only the viewer's side, beside its name", () => {
  const markup = scoreMarkup([3, 4], "Score", ["BLUE", "RED"], 1);
  assert.match(markup, /<span class="red yours">4<small>RED<em> · YOU<\/em><\/small>/);
  assert.match(markup, /<span class="blue">3<small>BLUE<\/small>/);
  assert.equal(markup.match(/YOU/g)?.length, 1);
});
