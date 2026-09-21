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
const featuredMetrics = [
  "kills",
  "busiestMinute",
  "longestLife",
  "bestLife",
  "damage",
  "rank",
] as const;
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
    kills: "Eliminations",
    damage: "Hull damage dealt",
    bestLife: "Best killing spree",
    rank: "Highest rank",
    busiestMinute: "Busiest minute",
    longestLife: "Longest life",
    multikill: "Biggest multikill",
    clutchKills: "Clutch kills",
    revengeKills: "Revenge kills",
    posthumousKills: "Beyond the grave",
    mineKills: "Mine kills",
    coverDestroyed: "Cover demolished",
    pickups: "Pickups grabbed",
  };
  const hints: Record<Metric, string> = {
    kills: "enemy tanks wrecked",
    damage: "enemy hull only · no overkill",
    bestLife: "kills while alive in one life",
    rank: "your peak across all lives",
    busiestMinute: "most kills in any rolling 60s",
    longestLife: "alive time · pauses excluded",
    multikill: "most kills in any rolling 5s",
    clutchKills: "kills at 25% hull or less",
    revengeKills: "taking out your last killer",
    posthumousKills: "kills by a previous life's ordnance",
    mineKills: "kills from mine explosions",
    coverDestroyed: "destructible objects you finished",
    pickups: "ammo and power-ups collected",
  };
  const format = (metric: Metric, value: number) =>
    metric === "rank"
      ? (RANKS[Math.min(RANKS.length - 1, Math.floor(value))]?.name ?? "Rookie")
      : metric === "longestLife"
        ? duration(value)
        : value.toLocaleString("en-US");
  const record = (metric: Metric) =>
    `${records.improved.has(metric) ? "★ NEW BEST" : "BEST"} ${format(metric, records.best[metric])}`;
  const feats = combatFeats(stats, combat.shots, combat.directHits);
  const detailMetrics = metrics.filter(
    (metric) => !featuredMetrics.some((featured) => featured === metric),
  );
  const detail = (label: string, value: string, hint: string, best = "", improved = false) =>
    `<div class="recap-detail${improved ? " is-record" : ""}" title="${hint}"><dt>${label}<small>${hint}</small></dt><dd>${value}${best ? `<small>${best}</small>` : ""}</dd></div>`;
  const markup = `<div class="recap-feats">${feats.length ? feats.map((feat) => `<div><b>★ ${feat.title}</b><span>${feat.detail}</span></div>`).join("") : `<div><b>${stats.kills ? "TRACKS DOWN. CHIN UP." : "A GLORIOUS PILE OF SCRAP"}</b><span>${stats.kills ? "Every wreck has a story. Here's yours." : "The next battle is your comeback story."}</span></div>`}</div>
    <div class="recap-heading">YOUR BATTLE REPORT<span>${records.improved.size ? `★ ${records.improved.size} NEW PERSONAL BEST${records.improved.size === 1 ? "" : "S"}` : ""}</span></div>
    <dl class="recap-stats">${featuredMetrics
      .map(
        (metric) => `<div class="recap-stat${records.improved.has(metric) ? " is-record" : ""}">
      <dt>${labels[metric]}</dt><dd>${format(metric, stats[metric])}<small>${hints[metric]}</small></dd>
      <span>${record(metric)}</span>
    </div>`,
      )
      .join("")}</dl>
    <dl class="recap-details">
      ${detail("Kills / minute", simulation.elapsed > 0 ? ((stats.kills * 60) / simulation.elapsed).toFixed(1) : "—", "round average · includes respawn time")}
      ${detail("Direct hit rate", combat.shots ? `${Math.round((combat.directHits / combat.shots) * 100)}%` : "—", `${combat.directHits} / ${combat.shots} projectiles · excludes splash hits`)}
      ${detailMetrics.map((metric) => detail(labels[metric], format(metric, stats[metric]), hints[metric], record(metric), records.improved.has(metric))).join("")}
      ${detail("Hull damage taken", Math.round(combat.damageTaken).toLocaleString("en-US"), "actual hull lost across all lives")}
      ${detail("Shield saved you", Math.round(combat.shieldAbsorbed).toLocaleString("en-US"), "damage absorbed by your shields")}
      ${detail("Time in the mayhem", duration(simulation.elapsed), `${simulation.human.deaths} wrecks · pauses excluded`)}
    </dl>
    <p class="recap-note">${simulation.difficulty.toUpperCase()} · Records for this mode &amp; map · ${records.persisted ? (records.established ? "Saved on this browser" : "First records set. Beat them next round!") : "Records could not be saved"}</p>`;
  finishedRecaps.set(simulation.match, markup);
  return markup;
}
