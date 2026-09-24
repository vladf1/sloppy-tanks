import { longestLife } from "./combat-record";
import type { Simulation } from "./simulation";
import type { Match } from "./types";
import { RANKS } from "./veterancy";

const metrics = [
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
] as const;
const featuredMetrics = ["kills", "damage", "longestLife", "bestLife", "rank"] as const;
type Metric = (typeof metrics)[number];
export type RecapStats = Record<Metric, number>;
type StorageAccess = Pick<Storage, "getItem" | "setItem">;
const finishedRecaps = new WeakMap<Match, string>();

export function savePersonalBests(storage: StorageAccess, key: string, stats: RecapStats) {
  const best = { ...stats };
  const improved = new Set<Metric>();
  let established = false;
  try {
    const saved: unknown = JSON.parse(storage.getItem(key) ?? "null");
    if (saved && typeof saved === "object") {
      for (const metric of metrics) {
        const previous: unknown = (saved as Record<string, unknown>)[metric];
        if (typeof previous === "number" && Number.isFinite(previous) && previous >= 0) {
          established = true;
          best[metric] = Math.max(previous, stats[metric]);
          if (stats[metric] > previous) {
            improved.add(metric);
          }
        }
      }
    }
  } catch {
    // Malformed or unavailable storage must not prevent the results screen.
  }
  let persisted = false;
  try {
    storage.setItem(key, JSON.stringify(best));
    persisted = true;
  } catch {
    // Private browsing and storage quotas can make records session-only.
  }
  return { best, improved, established, persisted };
}

export function recapStats(simulation: Simulation): RecapStats {
  const tank = simulation.human;
  const combat = simulation.combatRecord;
  return {
    kills: tank.kills,
    damage: Math.round(tank.damageDealt),
    bestLife: tank.bestLifeKills,
    rank: tank.highestRank,
    busiestMinute: combat.busiestMinute,
    longestLife: Math.floor(longestLife(simulation)),
    multikill: combat.multikill,
    clutchKills: combat.clutchKills,
    revengeKills: combat.revengeKills,
    posthumousKills: combat.posthumousKills,
    mineKills: combat.mineKills,
    coverDestroyed: combat.coverDestroyed,
    pickups: combat.pickups,
  };
}

export function combatFeats(stats: RecapStats, shots: number, directHits: number) {
  const feats: { title: string; detail: string }[] = [];
  if (stats.multikill >= 3) {
    feats.push({ title: "ONE-TANK ARMY", detail: `${stats.multikill} kills in five seconds` });
  }
  if (stats.clutchKills >= 2) {
    feats.push({
      title: "TOO ANGRY TO DIE",
      detail: `${stats.clutchKills} kills at 25% hull or less`,
    });
  }
  if (stats.posthumousKills > 0) {
    feats.push({
      title: "DEAD BUT DANGEROUS",
      detail: `${stats.posthumousKills} kill${stats.posthumousKills === 1 ? "" : "s"} from a previous life's ordnance`,
    });
  }
  if (stats.mineKills >= 2) {
    feats.push({ title: "MIND YOUR STEP", detail: `${stats.mineKills} mine-blast kills` });
  }
  if (stats.revengeKills > 0) {
    feats.push({
      title: "NOTHING PERSONAL",
      detail: `${stats.revengeKills} score${stats.revengeKills === 1 ? "" : "s"} settled`,
    });
  }
  if (stats.coverDestroyed >= 10) {
    feats.push({
      title: "URBAN REDEVELOPMENT",
      detail: `${stats.coverDestroyed} pieces of cover demolished`,
    });
  }
  if (shots >= 20 && directHits / shots >= 0.65) {
    feats.push({
      title: "SURGICAL STRIKES",
      detail: `${Math.round((directHits / shots) * 100)}% direct hit rate across ${shots} projectiles`,
    });
  }
  if (stats.longestLife >= 180) {
    feats.push({
      title: "HARD TO KILL",
      detail: `${duration(stats.longestLife)} without getting wrecked`,
    });
  }
  if (stats.busiestMinute >= 5) {
    feats.push({
      title: "RUSH HOUR",
      detail: `${stats.busiestMinute} kills in your busiest minute`,
    });
  }
  if (stats.rank === 3) {
    feats.push({ title: "LOCAL LEGEND", detail: "Reached Heroic rank" });
  }
  return feats.slice(0, 3);
}

function duration(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;
}

export function recapMarkup(simulation: Simulation): string {
  const cached = finishedRecaps.get(simulation.match);
  if (cached) {
    return cached;
  }
  const stats = recapStats(simulation);
  const combat = simulation.combatRecord;
  const key = `sloppy-records-v1:${simulation.gameMode}:${simulation.mapName}:${simulation.difficulty}`;
  const storage: StorageAccess = {
    getItem: (name) => localStorage.getItem(name),
    setItem: (name, value) => localStorage.setItem(name, value),
  };
  const records = savePersonalBests(storage, key, stats);
  const labels: Record<Metric, string> = {
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
  const hints: Record<Metric, string> = {
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
  const format = (metric: Metric, value: number) =>
    metric === "rank"
      ? (RANKS[Math.min(RANKS.length - 1, Math.floor(value))]?.name ?? "Rookie")
      : metric === "longestLife"
        ? duration(value)
        : value.toLocaleString("en-US");
  const tile = (label: string, value: string, hint: string, footer = "", improved = false) =>
    `<div class="recap-stat${improved ? " is-record" : ""}" title="${hint}"><dt>${label}</dt><dd>${value}</dd>${footer ? `<span>${footer}</span>` : ""}</div>`;
  // A tile only mentions its record when it was beaten or is still out of reach.
  const recordFooter = (metric: Metric) =>
    records.improved.has(metric)
      ? "★ NEW BEST"
      : records.best[metric] > stats[metric]
        ? `BEST ${format(metric, records.best[metric])}`
        : "";
  const hitRate = combat.shots ? `${Math.round((combat.directHits / combat.shots) * 100)}%` : "—";
  const extras: { label: string; value: string; hint: string; improved?: boolean }[] = [
    ...metrics
      .filter(
        (metric) => !featuredMetrics.some((featured) => featured === metric) && stats[metric] > 0,
      )
      .map((metric) => ({
        label: labels[metric],
        value: format(metric, stats[metric]),
        hint: hints[metric],
        improved: records.improved.has(metric),
      })),
    ...(combat.damageTaken >= 1
      ? [
          {
            label: "Damage taken",
            value: Math.round(combat.damageTaken).toLocaleString("en-US"),
            hint: "Hull lost across all lives",
          },
        ]
      : []),
    ...(combat.shieldAbsorbed >= 1
      ? [
          {
            label: "Shield absorbed",
            value: Math.round(combat.shieldAbsorbed).toLocaleString("en-US"),
            hint: "Damage your shields soaked up",
          },
        ]
      : []),
    { label: "Time played", value: duration(simulation.elapsed), hint: "Pauses excluded" },
  ];
  const feats = combatFeats(stats, combat.shots, combat.directHits);
  const bests = records.improved.size;
  // Records are kept per mode, map and difficulty; only mention them when something happened.
  const difficulty = simulation.difficulty[0].toUpperCase() + simulation.difficulty.slice(1);
  const note = !records.persisted
    ? "Personal bests couldn't be saved in this browser"
    : bests
      ? `<b>★ ${bests} NEW PERSONAL BEST${bests === 1 ? "" : "S"}</b> on ${simulation.mapName} · ${difficulty}`
      : "";
  const markup = `${feats.length ? `<div class="recap-feats">${feats.map((feat) => `<div title="${feat.detail}"><b>★ ${feat.title}</b> ${feat.detail}</div>`).join("")}</div>` : ""}
    <dl class="recap-stats">${featuredMetrics
      .map((metric) =>
        tile(
          labels[metric],
          format(metric, stats[metric]),
          hints[metric],
          recordFooter(metric),
          records.improved.has(metric),
        ),
      )
      .join(
        "",
      )}${tile("Accuracy", hitRate, "Direct hits / projectiles fired, splash excluded", `${combat.directHits} / ${combat.shots} HITS`)}</dl>
    <dl class="recap-details">${extras
      .map(
        (extra) =>
          `<div class="recap-detail${extra.improved ? " is-record" : ""}" title="${extra.hint}${extra.improved ? " · new personal best" : ""}"><dt>${extra.label}</dt><dd>${extra.improved ? "★ " : ""}${extra.value}</dd></div>`,
      )
      .join("")}</dl>
    <p class="recap-note">${note}</p>`;
  finishedRecaps.set(simulation.match, markup);
  return markup;
}
