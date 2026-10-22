import assert from "node:assert/strict";
import { test } from "node:test";
import type { HudState } from "../src/game/engine-api";
import { hudMarkup, menuMarkup, ordinal, scoreMarkup } from "../src/game/ui-markup";

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
  assert.match(markup, /<dt>Kills \/ deaths<\/dt><dd>2 \/ 1<small>this battle<\/small><\/dd>/);
  assert.match(markup, /<dt>Difficulty<\/dt><dd>Normal<\/dd>/);
  // The line under the title says who leads in words and what the team still needs.
  assert.match(
    markup,
    /class="dialog-standing" data-tone="ahead">Your team leads by 1 · \d+ kills? to win</,
  );
  // The team rank says what it ranks by.
  assert.match(markup, /<dt>Team rank<\/dt><dd>1st of 1<small>by kills<\/small><\/dd>/);
});

test("a battle ended early is called by where the team stood, never won", () => {
  const early = menuMarkup(teamBattle({ phase: "results", endedEarly: true }), null);
  assert.match(early, /<h2 id="results-title">LEAD HELD<\/h2>/);
  assert.match(early, /<span class="early-tag">ENDED EARLY<\/span>/);
  assert.match(early, /data-outcome="ended-ahead"/);
  assert.doesNotMatch(early, /VICTORY|ENDED BY YOU/);
  // The line is about the team alone; the viewer's deaths are on the board.
  assert.match(early, /Your team led by 1 kill when you ended the battle\.</);
  const full = menuMarkup(teamBattle({ phase: "results" }), null);
  assert.doesNotMatch(full, /ENDED BY YOU/);
  assert.match(full, /Your team took the round by 1\.</);
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

test("the ammo rack shows every round's own silhouette, never a locked paragraph", () => {
  const markup = hudMarkup();
  // Each of the five slots carries its round's icon and a lock for when it is empty.
  for (const weapon of ["standard", "spread", "rocket", "ricochet", "piercing"]) {
    assert.match(markup, new RegExp(`id="ammo-${weapon}"[^]*?<use href="#hud-${weapon}" />`));
  }
  assert.equal(markup.match(/class="ammo-lock"/g)?.length, 5);
  assert.doesNotMatch(markup, /LOCKED|Grab ammo crates/);
});

test("the respawn card's copy holds for mouse and touch alike", () => {
  const markup = menuMarkup(teamBattle({ phase: "playing", alive: false }), null);
  assert.match(markup, /Choose your next tank<small>It rolls out when the timer ends<\/small>/);
  assert.doesNotMatch(markup, /[Cc]lick|[Tt]ap /);
});
