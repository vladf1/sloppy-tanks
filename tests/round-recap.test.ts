// The report's words (tiles, standout, highlights, notes, board order and MVP) come from
// the engine and are covered by `crates/web/src/battle_report.rs`; the recap's statistics,
// feats and personal bests by `crates/core/tests/integration/round_recap.rs`. This checks
// how the page lays the report out.
import { test } from "node:test";
import assert from "node:assert/strict";
import { recapMarkup, scoreboardMarkup } from "../src/game/round-recap";
import type {
  BattleReport,
  RecapMetric,
  RecapState,
  ReportTile,
  ScoreboardRow,
} from "../src/game/engine-api";

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

const stat = (label: string, value: string, more: Partial<ReportTile> = {}): ReportTile => ({
  label,
  hint: `${label} hint`,
  value,
  footer: "",
  record: false,
  standout: false,
  ...more,
});

function report(overrides: Partial<BattleReport> = {}): BattleReport {
  return {
    outcome: "won",
    title: "VICTORY",
    lede: "Your team took the round by 1.",
    tiles: [
      stat("Kills", "3"),
      stat("Damage dealt", "1,234"),
      stat("Best spree", "0"),
      stat("Longest life", "1:35"),
      stat("Accuracy", "—", { footer: "0 / 0 HITS" }),
    ],
    progress: { text: "Ties your personal best", record: true, meter: null },
    details: [],
    note: null,
    board: {
      teams: [
        { label: "Blue team", tanks: [] },
        { label: "Red team", tanks: [] },
      ],
      mvp: null,
    },
    ...overrides,
  };
}

function recap(overrides: Partial<BattleReport> = {}, feats: RecapState["feats"] = []): RecapState {
  return {
    stats: zero(),
    best: zero(),
    improved: [],
    persisted: true,
    feats,
    shots: 0,
    directHits: 0,
    damageTaken: 0,
    shieldAbsorbed: 0,
    rankNames: ["Rookie", "Veteran", "Elite", "Heroic"],
    report: report(overrides),
  };
}

test("the hero tile carries the kills, the progress line and the tank", () => {
  const markup = recapMarkup(recap());
  assert.match(
    markup,
    /<div class="recap-stat" title="Kills hint"><dt>Kills<\/dt><dd>3<\/dd><em class="recap-progress is-record"><b>★ Ties your personal best<\/b><\/em><i class="recap-tank" aria-hidden="true"><\/i><\/div>/,
  );
  assert.match(markup, /<dt>Damage dealt<\/dt><dd>1,234<\/dd><\/div>/);
  assert.match(markup, /<dt>Accuracy<\/dt><dd>—<\/dd><span>0 \/ 0 HITS<\/span>/);
  assert.doesNotMatch(markup, /NEW BEST|recap-feats|is-standout/);
  assert.match(markup, /<dl class="recap-details" hidden><\/dl>/);
  assert.match(markup, /<p class="recap-note"><\/p>/);
  const gap = recapMarkup(
    recap({ progress: { text: "4 short of your best · 11", record: false, meter: 7 / 11 } }),
  );
  assert.match(
    gap,
    /<em class="recap-progress"><i class="recap-meter" style="--share:0.636"><\/i><b>4 short of your best · 11<\/b><\/em>/,
  );
});

test("records are starred and the standout tile is lit", () => {
  const markup = recapMarkup(
    recap(
      {
        tiles: [
          stat("Kills", "5", { record: true }),
          stat("Damage dealt", "400", { footer: "BEST 900" }),
          stat("Best spree", "2", { footer: "NEW BEST", record: true, standout: true }),
          stat("Longest life", "0:40"),
          stat("Accuracy", "65%", { footer: "13 / 20 HITS" }),
        ],
        progress: { text: "NEW PERSONAL BEST", record: true, meter: null },
        details: [
          { label: "Mine kills", hint: "Mines · new personal best", value: "2", record: true },
        ],
        note: { record: "2 NEW PERSONAL BESTS", text: "on Pine Village · Hard" },
      },
      [{ title: "MIND YOUR STEP", detail: "2 mine-blast kills" }],
    ),
  );
  assert.match(
    markup,
    /recap-stat is-record" title="Kills hint"><dt>Kills<\/dt><dd>5<\/dd><em class="recap-progress is-record"><b>★ NEW PERSONAL BEST/,
  );
  assert.match(markup, /<dd>400<\/dd><span>BEST 900<\/span>/);
  assert.match(
    markup,
    /recap-stat is-record is-standout" title="Best spree hint"><dt>Best spree<\/dt><dd>2<\/dd><span>★ NEW BEST<\/span>/,
  );
  assert.equal(markup.match(/is-standout/g)?.length, 1);
  assert.match(
    markup,
    /<div class="recap-detail is-record" title="Mines · new personal best"><dt>Mine kills<\/dt><dd>★ 2<\/dd>/,
  );
  assert.match(markup, /<dl class="recap-details">/);
  assert.match(markup, /<b>★ MIND YOUR STEP<\/b> 2 mine-blast kills/);
  assert.match(markup, /<b>★ 2 NEW PERSONAL BESTS<\/b> on Pine Village · Hard<\/p>/);
});

test("a warning note shows plainly", () => {
  const markup = recapMarkup(
    recap({ note: { record: null, text: "Personal bests couldn't be saved in this browser" } }),
  );
  assert.match(
    markup,
    /<p class="recap-note">Personal bests couldn't be saved in this browser<\/p>/,
  );
});

const row = (id: number, name: string, team: 0 | 1, kills: number, deaths: number) =>
  ({ id, name, team, kills, deaths, kind: "balanced", human: id === 1 }) as ScoreboardRow;

test("the scoreboard lists each team in the engine's order, lights the viewer and its MVP", () => {
  const rows = [
    row(1, "You", 1, 2, 1),
    row(2, "Ace", 0, 2, 0),
    row(3, "Bob <b>", 0, 0, 2),
    row(4, "Cat", 1, 3, 4),
  ];
  const board: BattleReport["board"] = {
    teams: [
      { label: "Blue team", tanks: [2, 3] },
      { label: "Red team", tanks: [4, 1] },
    ],
    mvp: 4,
  };
  const markup = scoreboardMarkup(rows, board, 1, ["BLUE", "RED"], [2, 5]);
  assert.match(markup, /aria-label="Blue team, 2"><header><b>BLUE<\/b><span><\/span>/);
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
  assert.match(markup, /fewest deaths breaks a tie/);
  assert.doesNotMatch(markup, /board-mvp/);
  // No MVP before anyone scores, and no rule to explain it.
  assert.doesNotMatch(
    scoreboardMarkup(rows, { ...board, mvp: null }, 1, ["BLUE", "RED"], [0, 0]),
    /board-key/,
  );
});
