import type { HudState, RecapMetric, RecapState, ScoreboardRow } from "./engine-api";

/** Each metric's label and, on hover, its explanation, so the report itself reads at a
 * glance. In report order; the engine computes the values, feats and personal bests. */
const METRICS: Record<RecapMetric, { label: string; hint: string }> = {
  kills: { label: "Kills", hint: "Enemy tanks destroyed" },
  damage: { label: "Damage dealt", hint: "Enemy hull damage, no overkill" },
  bestLife: { label: "Best spree", hint: "Most kills in a single life" },
  rank: { label: "Top rank", hint: "Peak rank across all lives" },
  busiestMinute: { label: "Busiest minute", hint: "Most kills within any 60 seconds" },
  longestLife: { label: "Longest life", hint: "Longest time alive, pauses excluded" },
  multikill: { label: "Best multikill", hint: "Most kills within any 5 seconds" },
  clutchKills: { label: "Clutch kills", hint: "Kills at 25% hull or less" },
  revengeKills: { label: "Revenge kills", hint: "Kills on your last killer" },
  posthumousKills: { label: "Kills after death", hint: "Kills by a previous life's ordnance" },
  mineKills: { label: "Mine kills", hint: "Kills from mine explosions" },
  coverDestroyed: { label: "Cover destroyed", hint: "Destructible objects you finished" },
  pickups: { label: "Pickups", hint: "Ammo and power-ups collected" },
};
/** The hero tile and the tiles beside it; peak rank shows on the hero tile with the
 * tank, and the rest of the metrics list below only when they happened. */
const featuredMetrics: readonly RecapMetric[] = ["kills", "damage", "bestLife", "longestLife"];

/** How often the viewer's tank was destroyed this round, for the report's opening line. */
export function wreckLine(deaths: number): string {
  return deaths <= 0
    ? "Never destroyed"
    : deaths === 1
      ? "Destroyed once"
      : deaths === 2
        ? "Destroyed twice"
        : `Destroyed ${deaths} times`;
}

/** "BLUE" as the report says it: "Blue team". */
export function teamLabel(name: string): string {
  return `${name.charAt(0)}${name.slice(1).toLowerCase()} team`;
}

/** Where a team battle stands for the viewer's `team`: who leads and by how much. */
export function standingLine(
  scores: readonly number[],
  team: number,
  names: readonly string[],
): string {
  const [blue, red] = scores;
  if (blue === red) {
    return `Level at ${blue} each`;
  }
  const leader = blue > red ? 0 : 1;
  const margin = Math.abs(blue - red);
  return `${leader === team ? "Your team" : teamLabel(names[leader])} leads by ${margin}`;
}

export function duration(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;
}

/** The battle report's feats, stat tiles and details for a finished round. Kills comes
 * first: the report draws it as the hero tile. */
export function recapMarkup(
  hud: Pick<HudState, "elapsed" | "mapName" | "difficulty">,
  recap: RecapState,
): string {
  const { stats, best } = recap;
  const improved = new Set(recap.improved);
  const format = (metric: RecapMetric, value: number) =>
    metric === "rank"
      ? (recap.rankNames[Math.min(recap.rankNames.length - 1, Math.floor(value))] ?? "Rookie")
      : metric === "longestLife"
        ? duration(value)
        : value.toLocaleString("en-US");
  const tile = (label: string, value: string, hint: string, footer = "", isRecord = false) =>
    `<div class="recap-stat${isRecord ? " is-record" : ""}" title="${hint}"><dt>${label}</dt><dd>${value}</dd>${footer ? `<span>${footer}</span>` : ""}</div>`;
  // The hero tile carries the tank (drawn in by the page) and the peak rank it reached.
  const rankName = format("rank", stats.rank);
  const hero = (markup: string) =>
    markup.replace(
      /<\/div>$/,
      `<em class="recap-rank" title="${METRICS.rank.hint}">${improved.has("rank") ? "★ " : ""}PEAK RANK · ${rankName.toUpperCase()}</em><i class="recap-tank" aria-hidden="true"></i></div>`,
    );
  // A tile only mentions its record when it was beaten or is still out of reach.
  const recordFooter = (metric: RecapMetric) =>
    improved.has(metric)
      ? "★ NEW BEST"
      : best[metric] > stats[metric]
        ? `BEST ${format(metric, best[metric])}`
        : "";
  const hitRate = recap.shots ? `${Math.round((recap.directHits / recap.shots) * 100)}%` : "—";
  const extras: { label: string; value: string; hint: string; improved?: boolean }[] = [
    ...(Object.keys(METRICS) as RecapMetric[])
      .filter(
        (metric) => !featuredMetrics.includes(metric) && metric !== "rank" && stats[metric] > 0,
      )
      .map((metric) => ({
        ...METRICS[metric],
        value: format(metric, stats[metric]),
        improved: improved.has(metric),
      })),
    ...(recap.damageTaken >= 1
      ? [
          {
            label: "Damage taken",
            value: Math.round(recap.damageTaken).toLocaleString("en-US"),
            hint: "Hull lost across all lives",
          },
        ]
      : []),
    ...(recap.shieldAbsorbed >= 1
      ? [
          {
            label: "Shield absorbed",
            value: Math.round(recap.shieldAbsorbed).toLocaleString("en-US"),
            hint: "Damage your shields soaked up",
          },
        ]
      : []),
  ];
  const bests = improved.size;
  // Records are kept per mode, map and difficulty; only mention them when something happened.
  const difficulty = hud.difficulty[0].toUpperCase() + hud.difficulty.slice(1);
  const note = !recap.persisted
    ? "Personal bests couldn't be saved in this browser"
    : bests
      ? `<b>★ ${bests} NEW PERSONAL BEST${bests === 1 ? "" : "S"}</b> on ${hud.mapName} · ${difficulty}`
      : "";
  return `${recap.feats.length ? `<div class="recap-feats">${recap.feats.map((feat) => `<div title="${feat.detail}"><b>★ ${feat.title}</b> ${feat.detail}</div>`).join("")}</div>` : ""}
    <dl class="recap-stats">${featuredMetrics
      .map((metric, index) => {
        const markup = tile(
          METRICS[metric].label,
          format(metric, stats[metric]),
          METRICS[metric].hint,
          recordFooter(metric),
          improved.has(metric),
        );
        return index === 0 ? hero(markup) : markup;
      })
      .join(
        "",
      )}${tile("Accuracy", hitRate, "Direct hits / projectiles fired, splash excluded", `${recap.directHits} / ${recap.shots} HITS`)}</dl>
    <dl class="recap-details">${extras
      .map(
        (extra) =>
          `<div class="recap-detail${extra.improved ? " is-record" : ""}" title="${extra.hint}${extra.improved ? " · new personal best" : ""}"><dt>${extra.label}</dt><dd>${extra.improved ? "★ " : ""}${extra.value}</dd></div>`,
      )
      .join("")}</dl>
    <p class="recap-note">${note}</p>`;
}

const HTML_ESCAPES: Record<string, string> = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
};

/** Player names are typed by players: shown as text, never as markup. */
export function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (char) => HTML_ESCAPES[char]);
}

/** The scoreboard's order: most kills first, then fewest wrecks, then by name. */
function byScore(a: ScoreboardRow, b: ScoreboardRow): number {
  return b.kills - a.kills || a.deaths - b.deaths || a.name.localeCompare(b.name);
}

/** Where the viewer stands on their own team's scoreboard (by kills, then fewest
 * wrecks): 1-based place and the team's size, or null when the viewer is not listed. */
export function teamPlace(
  rows: readonly ScoreboardRow[],
  viewerId: number,
): { place: number; of: number } | null {
  const viewer = rows.find((row) => row.id === viewerId);
  if (!viewer) {
    return null;
  }
  const team = rows.filter((row) => row.team === viewer.team).sort(byScore);
  return { place: team.indexOf(viewer) + 1, of: team.length };
}

/** The round's most valuable tank: the most kills, fewest wrecks breaking a tie; none
 * before anyone scores. */
export function mostValuable(rows: readonly ScoreboardRow[]): ScoreboardRow | null {
  const best = [...rows].sort(byScore)[0];
  return best && best.kills > 0 ? best : null;
}

/** The battle report's team scoreboard: each team's tanks by kills, with their wrecks,
 * the viewer's row lit and the MVP tagged in its row (no second MVP line repeats it).
 * `teamNames` and `scores` are in team order. */
export function scoreboardMarkup(
  rows: readonly ScoreboardRow[],
  viewerId: number,
  teamNames: readonly string[],
  scores: readonly number[],
): string {
  const mvp = mostValuable(rows);
  const team = (index: 0 | 1) => {
    const side = index === 0 ? "blue" : "red";
    const members = rows.filter((row) => row.team === index).sort(byScore);
    const lines = members
      .map((row) => {
        const classes = [row.id === viewerId ? "you" : "", row === mvp ? "mvp" : ""]
          .filter(Boolean)
          .join(" ");
        const tag =
          row === mvp
            ? `<span class="board-tag mvp-tag">MVP</span>`
            : row.id === viewerId
              ? `<span class="board-tag">YOU</span>`
              : "";
        return `<li${classes ? ` class="${classes}"` : ""}><span class="board-name">${escapeHtml(row.name)}</span>${tag}<b>${row.kills}</b><small>${row.deaths}</small></li>`;
      })
      .join("");
    return `<section class="board-team ${side}" aria-label="${teamLabel(teamNames[index])}"><header><b>${teamNames[index]}</b><span>${scores[index]}</span><i>K</i><i>D</i></header><ol>${lines}</ol></section>`;
  };
  return `<div class="results-board">${team(0)}${team(1)}</div>`;
}
