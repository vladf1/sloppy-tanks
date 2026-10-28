import type { BattleReport, RecapState, ReportTile, ScoreboardRow } from "./engine-api";

/** The HUD clock's `m:ss`. */
export function duration(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;
}

/** The battle report's feats, stat tiles and details for a finished round, in the
 * engine's words (`recap.report`). Kills comes first: the report draws it as the hero
 * tile, with the tank (drawn in by the page) and the kills measured against the best. */
export function recapMarkup(recap: RecapState): string {
  const { report } = recap;
  const tile = (stat: ReportTile, footer: string) =>
    `<div class="recap-stat${stat.record ? " is-record" : ""}${stat.standout ? " is-standout" : ""}" title="${stat.hint}"><dt>${stat.label}</dt><dd>${stat.value}</dd>${footer}</div>`;
  const [hero, ...small] = report.tiles;
  const tiles = [
    hero
      ? tile(
          hero,
          `${progressMarkup(report.progress)}<i class="recap-tank" aria-hidden="true"></i>`,
        )
      : "",
    ...small.map((stat) =>
      tile(stat, stat.footer ? `<span>${stat.record ? "★ " : ""}${stat.footer}</span>` : ""),
    ),
  ].join("");
  const details = report.details
    .map(
      (detail) =>
        `<div class="recap-detail${detail.record ? " is-record" : ""}" title="${detail.hint}"><dt>${detail.label}</dt><dd>${detail.record ? "★ " : ""}${detail.value}</dd></div>`,
    )
    .join("");
  const { note } = report;
  return `${recap.feats.length ? `<div class="recap-feats">${recap.feats.map((feat) => `<div title="${feat.detail}"><b>★ ${feat.title}</b> ${feat.detail}</div>`).join("")}</div>` : ""}
    <dl class="recap-stats">${tiles}</dl>
    <dl class="recap-details"${details ? "" : " hidden"}>${details}</dl>
    <p class="recap-note">${note ? (note.record ? `<b>★ ${note.record}</b> ${note.text}` : note.text) : ""}</p>`;
}

/** The hero tile's line under the kills: a meter toward the best when there is a gap,
 * a record or a tie named in gold. */
function progressMarkup({ text, record, meter }: BattleReport["progress"]): string {
  const bar =
    meter === null ? "" : `<i class="recap-meter" style="--share:${meter.toFixed(3)}"></i>`;
  return `<em class="recap-progress${record ? " is-record" : ""}">${bar}<b>${record ? "★ " : ""}${text}</b></em>`;
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

/** The battle report's team scoreboard: each team's tanks in the engine's order, with
 * their wrecks, the viewer's row lit and the MVP tagged in its row (no second MVP line
 * repeats it). `teamNames` and `scores` are in team order. */
export function scoreboardMarkup(
  rows: readonly ScoreboardRow[],
  board: BattleReport["board"],
  viewerId: number,
  teamNames: readonly string[],
  scores: readonly number[],
): string {
  const team = (index: 0 | 1) => {
    const side = index === 0 ? "blue" : "red";
    const lines = board.teams[index].tanks
      .flatMap((id) => rows.filter((row) => row.id === id))
      .map((row) => {
        const mvp = row.id === board.mvp;
        const classes = [row.id === viewerId ? "you" : "", mvp ? "mvp" : ""]
          .filter(Boolean)
          .join(" ");
        const tag = mvp
          ? `<span class="board-tag mvp-tag">MVP</span>`
          : row.id === viewerId && row.name !== "YOU"
            ? `<span class="board-tag">YOU</span>`
            : "";
        return `<li${classes ? ` class="${classes}"` : ""}><span class="board-name">${escapeHtml(row.name)}</span>${tag}<b>${row.kills}</b><small>${row.deaths}</small></li>`;
      })
      .join("");
    return `<section class="board-team ${side}" aria-label="${board.teams[index].label}, ${scores[index]}"><header><b>${teamNames[index]}</b><span></span><i>K</i><i>D</i></header><ol>${lines}</ol></section>`;
  };
  // The rule behind the MVP tag, so a tie on kills never looks arbitrary.
  const key =
    board.mvp !== null
      ? `<p class="board-key"><span class="board-tag mvp-tag">MVP</span>Most kills · fewest deaths breaks a tie</p>`
      : "";
  return `<div class="results-board">${team(0)}${team(1)}${key}</div>`;
}
