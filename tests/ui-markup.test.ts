import assert from "node:assert/strict";
import { test } from "node:test";
import type { BattleReport, HudState, PauseReport } from "../src/game/engine-api";
import { hudMarkup, menuMarkup, scoreMarkup } from "../src/game/ui-markup";

/** A team battle's HUD state, three kills to four, as the menus read it; at results with
 * the engine's `report`. */
function teamBattle(overrides: {
  phase: string;
  endedEarly?: boolean;
  alive?: boolean;
  report?: Partial<BattleReport>;
}): HudState {
  const human = {
    id: 1,
    kind: "balanced",
    vehicleName: "BRUISER",
    rankName: "Rookie",
    alive: overrides.alive ?? true,
    kills: 2,
  };
  return {
    match: {
      phase: overrides.phase,
      scores: [3, 4],
      endedEarly: overrides.endedEarly ?? false,
      round: 1,
    },
    gameMode: "team",
    endlessMatch: false,
    mapName: "Pine Village",
    difficulty: "normal",
    humanTeam: 1,
    scoreLimit: 100,
    teamNames: ["BLUE", "RED"],
    human,
    scoreboard: [{ id: 1, name: "YOU", kills: 2, deaths: 1 }],
    recap:
      overrides.phase === "results"
        ? {
            feats: [],
            report: {
              outcome: "won",
              title: "VICTORY",
              lede: "Your team took the round by 1.",
              tiles: [],
              progress: { text: "No kills this round", record: false, meter: 0 },
              details: [],
              note: null,
              board: {
                teams: [
                  { label: "Blue team", tanks: [] },
                  { label: "Red team", tanks: [1] },
                ],
                mvp: 1,
              },
              ...overrides.report,
            },
          }
        : null,
  } as unknown as HudState;
}

/** Pause's facts as the engine words them for `teamBattle`. */
const pause: PauseReport = {
  tally: null,
  standing: { text: "Your team leads by 1 · 96 kills to win", tone: "ahead" },
  facts: [
    { label: "Objective", value: "First to 100", note: "" },
    { label: "Clock", value: "4:44 left", note: "" },
    { label: "Difficulty", value: "Normal", note: "" },
    { label: "Your tank", value: "BRUISER", note: "Rookie" },
    { label: "Kills / deaths", value: "2 / 1", note: "this battle" },
    { label: "Team rank", value: "1st of 1", note: "by kills" },
  ],
};

test("pause leaves the score to the header's plates and lists the battle's facts", () => {
  const markup = menuMarkup(teamBattle({ phase: "paused" }), null, pause);
  assert.match(markup, /class="dialog-score"/);
  assert.doesNotMatch(markup, /KILL SHARE|share-bar|pause-objective/);
  assert.match(markup, /<dt>Objective<\/dt><dd>First to 100<\/dd>/);
  assert.match(markup, /<dt>Kills \/ deaths<\/dt><dd>2 \/ 1<small>this battle<\/small><\/dd>/);
  assert.match(markup, /<dt>Your tank<\/dt><dd>BRUISER<small>Rookie<\/small><\/dd>/);
  // The line under the title says who leads in words and what the team still needs.
  assert.match(
    markup,
    /class="dialog-standing" data-tone="ahead">Your team leads by 1 · 96 kills to win</,
  );
  assert.match(markup, /<dt>Team rank<\/dt><dd>1st of 1<small>by kills<\/small><\/dd>/);
});

test("a solo pause tallies the run above its facts and waits in words", () => {
  const solo = { ...teamBattle({ phase: "paused" }), gameMode: "solo" } as HudState;
  const markup = menuMarkup(solo, null, {
    ...pause,
    tally: { kills: 2, survived: "5:12" },
    standing: null,
  });
  assert.match(
    markup,
    /<div class="pause-objective solo"><div><b>2<\/b><small>KILLS<\/small><\/div><div><b>5:12<\/b><small>SURVIVED<\/small>/,
  );
  assert.match(markup, /<p class="dialog-lede">The battle waits until you resume\.<\/p>/);
  assert.doesNotMatch(markup, /dialog-standing|dialog-score/);
});

test("the battle report's banner shows the engine's outcome, title and sentence", () => {
  const early = menuMarkup(
    teamBattle({
      phase: "results",
      endedEarly: true,
      report: {
        outcome: "ended-ahead",
        title: "LEAD HELD",
        lede: "Your team led by 1 kill when you ended the battle.",
      },
    }),
    null,
  );
  assert.match(early, /<h2 id="results-title">LEAD HELD<\/h2>/);
  assert.match(early, /<span class="early-tag">ENDED EARLY<\/span>/);
  assert.match(early, /data-outcome="ended-ahead" data-team="red"/);
  assert.match(
    early,
    /<p class="dialog-lede">Your team led by 1 kill when you ended the battle\.</,
  );
  assert.match(early, /<li class="you mvp"><span class="board-name">YOU<\/span>/);
  const full = menuMarkup(teamBattle({ phase: "results" }), null);
  assert.doesNotMatch(full, /early-tag/);
  assert.match(full, /<h2 id="results-title">VICTORY<\/h2>/);
  assert.match(full, /Your team took the round by 1\.</);
});

test("a solo run's loss leads its sentence with the death that ended it", () => {
  const solo = {
    ...teamBattle({
      phase: "results",
      report: { outcome: "lost", title: "TANK DESTROYED", lede: "One more run?" },
    }),
    gameMode: "solo",
  } as HudState;
  const markup = menuMarkup(solo, {
    killer: "<Rex>",
    self: false,
    weapon: "Standard shell",
    icon: "standard",
    sentence: "<Rex> destroyed you with a standard shell.",
  });
  assert.match(
    markup,
    /<p class="dialog-lede">&lt;Rex&gt; destroyed you with a standard shell\. One more run\?<\/p>/,
  );
  assert.doesNotMatch(markup, /results-board/);
});

test("the respawn card names who destroyed the tank in plain words", () => {
  const markup = menuMarkup(teamBattle({ phase: "playing", alive: false }), {
    killer: "THUNDERCLAP",
    self: false,
    weapon: "Standard shell",
    icon: "standard",
    killerVehicle: "BIG RIG",
    killerTeam: 1,
    killerTally: "Their 3rd kill",
    sentence: "THUNDERCLAP destroyed you with a standard shell.",
  });
  assert.match(
    markup,
    /<div class="wrecked-by side-red"><small>DESTROYED BY<\/small><b>THUNDERCLAP<\/b>/,
  );
  assert.match(markup, /<span class="killer-tag">BIG RIG<\/span>/);
  assert.match(markup, /killer-tag-icon"[^]*?<\/svg>Their 3rd kill<\/span>/);
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
