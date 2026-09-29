// The recap's statistics, feats and personal bests are engine logic, covered by
// `crates/core/tests/round_recap.rs`; this checks how the battle report shows them.
import { test } from "node:test";
import assert from "node:assert/strict";
import { recapMarkup } from "../src/game/round-recap";
import type { RecapMetric, RecapState } from "../src/game/engine-api";

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
    established: false,
    persisted: true,
    feats: [],
    shots: 0,
    directHits: 0,
    damageTaken: 0,
    shieldAbsorbed: 0,
    rankNames: ["Rookie", "Veteran", "Elite", "Heroic"],
    recordsKey: "sloppy-records-v1:team:Pine Village:normal",
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
  assert.match(markup, /<dt>Top rank<\/dt><dd>Elite<\/dd>/);
  assert.match(markup, /<dt>Time played<\/dt><dd>2:05<\/dd>/);
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
      established: true,
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
