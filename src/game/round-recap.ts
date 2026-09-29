import type { HudState, RecapMetric, RecapState } from "./engine-api";

/** Report order; the engine computes the values, feats and personal bests. */
const metrics: readonly RecapMetric[] = [
  "kills",
  "damage",
  "bestLife",
  "rank",
  "busiestMinute",
  "longestLife",
  "multikill",
  "clutchKills",
  "revengeKills",
  "posthumousKills",
  "mineKills",
  "coverDestroyed",
  "pickups",
];
const featuredMetrics: readonly RecapMetric[] = [
  "kills",
  "damage",
  "longestLife",
  "bestLife",
  "rank",
];
const labels: Record<RecapMetric, string> = {
  kills: "Kills",
  damage: "Damage dealt",
  bestLife: "Best spree",
  rank: "Top rank",
  busiestMinute: "Busiest minute",
  longestLife: "Longest life",
  multikill: "Multikill",
  clutchKills: "Clutch kills",
  revengeKills: "Revenge kills",
  posthumousKills: "From the grave",
  mineKills: "Mine kills",
  coverDestroyed: "Cover wrecked",
  pickups: "Pickups",
};
// Explanations stay available on hover so the report itself reads at a glance.
const hints: Record<RecapMetric, string> = {
  kills: "Enemy tanks wrecked",
  damage: "Enemy hull damage, no overkill",
  bestLife: "Most kills in a single life",
  rank: "Peak rank across all lives",
  busiestMinute: "Most kills in any rolling 60s",
  longestLife: "Longest time alive, pauses excluded",
  multikill: "Most kills in any rolling 5s",
  clutchKills: "Kills at 25% hull or less",
  revengeKills: "Kills on your last killer",
  posthumousKills: "Kills by a previous life's ordnance",
  mineKills: "Kills from mine explosions",
  coverDestroyed: "Destructible objects you finished",
  pickups: "Ammo and power-ups collected",
};

export function duration(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;
}

/** The battle report's feats, stat tiles and details for a finished round. */
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
  // A tile only mentions its record when it was beaten or is still out of reach.
  const recordFooter = (metric: RecapMetric) =>
    improved.has(metric)
      ? "★ NEW BEST"
      : best[metric] > stats[metric]
        ? `BEST ${format(metric, best[metric])}`
        : "";
  const hitRate = recap.shots ? `${Math.round((recap.directHits / recap.shots) * 100)}%` : "—";
  const extras: { label: string; value: string; hint: string; improved?: boolean }[] = [
    ...metrics
      .filter((metric) => !featuredMetrics.includes(metric) && stats[metric] > 0)
      .map((metric) => ({
        label: labels[metric],
        value: format(metric, stats[metric]),
        hint: hints[metric],
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
    { label: "Time played", value: duration(hud.elapsed), hint: "Pauses excluded" },
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
      .map((metric) =>
        tile(
          labels[metric],
          format(metric, stats[metric]),
          hints[metric],
          recordFooter(metric),
          improved.has(metric),
        ),
      )
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
