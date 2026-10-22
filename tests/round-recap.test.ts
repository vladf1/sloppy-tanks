// The recap's statistics, feats and personal bests are engine logic, covered by
// `crates/core/tests/round_recap.rs`; this checks how the battle report shows them.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  mostValuable,
  recapMarkup,
  scoreboardMarkup,
  standingLine,
  teamLabel,
  wreckLine,
} from "../src/game/round-recap";
import type { RecapMetric, RecapState, ScoreboardRow } from "../src/game/engine-api";

const zero = (): Record<RecapMetric, number> => ({
  kills: 0,
  damage: 0,
  bestLife: 0,
  rank: 0,
  busiestMinute: 0,
  longestLife: 0,
  multikill: 0,
  clutchKills: 0,
  revengeKills: 0,
  posthumousKills: 0,
  mineKills: 0,
  coverDestroyed: 0,
  pickups: 0,
});

function recap(overrides: Partial<RecapState> = {}): RecapState {
  return {
    stats: zero(),
    best: zero(),
    improved: [],
    persisted: true,
    feats: [],
    shots: 0,
    directHits: 0,
    damageTaken: 0,
    shieldAbsorbed: 0,
    rankNames: ["Rookie", "Veteran", "Elite", "Heroic"],
    ...overrides,
  };
}
const round = { elapsed: 125, mapName: "Pine Village", difficulty: "hard" as const };

test("a first round reports its values without record callouts", () => {
  const stats = { ...zero(), kills: 3, damage: 1234, longestLife: 95, rank: 2 };
  const markup = recapMarkup(round, recap({ stats, best: stats }));
  assert.match(markup, /<dt>Kills<\/dt><dd>3<\/dd>/);
  assert.match(markup, /<dt>Damage dealt<\/dt><dd>1,234<\/dd>/);
  assert.match(markup, /<dt>Longest life<\/dt><dd>1:35<\/dd>/);
  // Peak rank rides on the kills tile; time played is not a stat worth a cell.
  assert.match(markup, /PEAK RANK · ELITE/);
  assert.doesNotMatch(markup, /Top rank|Time played/);
  assert.match(markup, /<dt>Accuracy<\/dt><dd>—<\/dd>/);
  assert.doesNotMatch(markup, /NEW BEST|BEST |recap-feats/);
  assert.match(markup, /<p class="recap-note"><\/p>/);
});

test("improved records are starred and unreached records show the best", () => {
  const stats = { ...zero(), kills: 5, damage: 400, mineKills: 2 };
  const best = { ...zero(), kills: 5, damage: 900, mineKills: 2 };
  const markup = recapMarkup(
    round,
    recap({
      stats,
      best,
      improved: ["kills", "mineKills"],
      shots: 20,
      directHits: 13,
      feats: [{ title: "MIND YOUR STEP", detail: "2 mine-blast kills" }],
    }),
  );
  assert.match(
    markup,
    /recap-stat is-record" title="Enemy tanks wrecked"><dt>Kills<\/dt><dd>5<\/dd><span>★ NEW BEST/,
  );
  assert.match(markup, /<dd>400<\/dd><span>BEST 900<\/span>/);
  assert.match(markup, /recap-detail is-record[^>]*><dt>Mine kills<\/dt><dd>★ 2<\/dd>/);
  assert.match(markup, /<dd>65%<\/dd><span>13 \/ 20 HITS<\/span>/);
  assert.match(markup, /<b>★ MIND YOUR STEP<\/b> 2 mine-blast kills/);
  assert.match(markup, /★ 2 NEW PERSONAL BESTS<\/b> on Pine Village · Hard/);
});

test("storage that refuses records says so", () => {
  const markup = recapMarkup(round, recap({ persisted: false, damageTaken: 250.4 }));
  assert.match(markup, /Personal bests couldn't be saved in this browser/);
  assert.match(markup, /<dt>Damage taken<\/dt><dd>250<\/dd>/);
});

test("the report counts wrecks in words and names the winning team plainly", () => {
  assert.deepEqual([0, 1, 2, 5].map(wreckLine), [
    "Never wrecked",
    "Wrecked once",
    "Wrecked twice",
    "Wrecked 5 times",
  ]);
  assert.equal(teamLabel("BLUE"), "Blue team");
});

const row = (id: number, name: string, team: 0 | 1, kills: number, deaths: number) =>
  ({ id, name, team, kills, deaths, kind: "balanced", human: id === 1 }) as ScoreboardRow;

test("the scoreboard lists each team by kills, lights the viewer and crowns one MVP", () => {
  const rows = [
    row(1, "You", 1, 2, 1),
    row(2, "Ace", 0, 2, 0),
    row(3, "Bob <b>", 0, 0, 2),
    row(4, "Cat", 1, 3, 4),
  ];
  // Most kills wins; fewer wrecks breaks a tie.
  assert.equal(mostValuable(rows)?.name, "Cat");
  assert.equal(mostValuable([row(5, "A", 0, 1, 2), row(6, "B", 1, 1, 0)])?.name, "B");
  assert.equal(mostValuable([row(5, "A", 0, 0, 0)]), null);
  const markup = scoreboardMarkup(rows, 1, ["BLUE", "RED"], [2, 5]);
  assert.match(markup, /<header><b>BLUE<\/b><span>2<\/span>/);
  assert.match(
    markup,
    /Ace<\/span><b>2<\/b><small>0<\/small><\/li><li><span class="board-name">Bob &lt;b&gt;/,
  );
  assert.match(
    markup,
    /<li class="mvp"><span class="board-name">Cat<\/span><span class="board-tag mvp-tag">MVP/,
  );
  assert.match(
    markup,
    /<li class="you"><span class="board-name">You<\/span><span class="board-tag">YOU/,
  );
  assert.match(markup, /board-mvp red"><span>★ MVP<\/span><b>Cat<\/b><small>3 kills · 4 wrecks/);
});

test("pause names who leads and by how much, from the viewer's side", () => {
  const names = ["BLUE", "RED"];
  assert.equal(standingLine([3, 4], 1, names), "Your team leads by 1");
  assert.equal(standingLine([7, 2], 1, names), "Blue team leads by 5");
  assert.equal(standingLine([2, 2], 0, names), "Level at 2 each");
});
