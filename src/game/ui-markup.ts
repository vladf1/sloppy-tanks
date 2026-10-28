import { escapeHtml, recapMarkup, scoreboardMarkup } from "./round-recap";
import { AMMO_OPTIONS } from "./ammo-options";
import type { HudState, PauseReport } from "./engine-api";
import type { DeathReport } from "./hud-feedback";
import { CLOSE_ICON, SETTINGS_BUTTON } from "./settings-dialog";

/** The HUD's icons, drawn once and placed with `<use>`: the ammunition and kill-feed
 * weapons, the corner buttons and the mine. Shapes rather than text glyphs, since small
 * font sets such as Tesla's browser lack symbols and draw missing-glyph boxes. Every
 * weapon points right, the way the kill feed reads from killer to victim. */
const HUD_ICONS = `<svg class="icon-sprite" aria-hidden="true" focusable="false">
<symbol id="hud-standard" viewBox="0 0 24 24"><path d="M5 9h8.6c3.2 0 6 1.4 7.9 3-1.9 1.6-4.7 3-7.9 3H5zM2 8.4h2v7.2H2z"/></symbol>
<symbol id="hud-spread" viewBox="0 0 24 24"><path d="M4 10.6h7.3c2.3 0 4.2.6 5.6 1.4-1.4.8-3.3 1.4-5.6 1.4H4zM5.1 3.6l6.8 2.6c2.2.8 3.8 2 4.8 3.2-1.6.3-3.6.2-5.7-.6L4.2 6.2zM4.2 17.8l6.8-2.6c2.1-.8 4.1-.9 5.7-.6-1 1.2-2.6 2.4-4.8 3.2l-6.8 2.6z"/></symbol>
<symbol id="hud-rocket" viewBox="0 0 24 24"><path d="M6 10h8l1.6-1.4h1.2c2.4 0 4.4 1.6 5.7 3.4-1.3 1.8-3.3 3.4-5.7 3.4h-1.2L14 14H6zM7 10 4 5h3.4L11 10zM7 14l-3 5h3.4L11 14zM1.5 11h3.5v2H1.5z"/></symbol>
<symbol id="hud-ricochet" viewBox="0 0 24 24"><path d="M2.6 5.4 8.4 15l5.2-7.6 3.5 5.1 1.6-1.1.9 6.3-5.9-2.2 1.6-1.1-1.7-2.5-5.3 7.7L.7 6.6z"/></symbol>
<symbol id="hud-piercing" viewBox="0 0 24 24"><path d="M2 10.8h13.2V7.4L22.5 12l-7.3 4.6v-3.4H2zM8.2 3h2.6v6.3H8.2zM8.2 14.7h2.6V21H8.2z"/></symbol>
<symbol id="hud-mine" viewBox="0 0 24 24"><path d="M12 6.2a6.3 6.3 0 1 1 0 12.6 6.3 6.3 0 0 1 0-12.6zM10.9 2.4h2.2v3.2h-2.2zM10.9 19.4h2.2v3.2h-2.2zM2 11.4h3.2v2.2H2zM18.8 11.4H22v2.2h-3.2zM4.3 5.3l1.6-1.6 2.3 2.3-1.6 1.6zM15.8 17l1.6-1.6 2.3 2.3-1.6 1.6zM4.3 19.7 6.6 17.4l1.6 1.6-2.3 2.3zM15.8 8l2.3-2.3 1.6 1.6-2.3 2.3z"/></symbol>
<symbol id="hud-drum" viewBox="0 0 24 24"><path fill-rule="evenodd" d="M6 3h12l1 2.2v13.6L18 21H6l-1-2.2V5.2zM5 8h14v1.6H5zm0 6.4h14V16H5z"/></symbol>
<symbol id="hud-blast" viewBox="0 0 24 24"><path d="m12 1.5 2.3 5.6 5.4-2.7-1.9 5.8 5.7 1.8-5.6 2.4 2.8 5.3-5.8-1.8-1.7 5.8-2.4-5.5-5.4 2.8 1.8-5.8L1.5 13l5.6-2.4-2.7-5.3 5.7 1.8z"/></symbol>
<symbol id="hud-eye" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round" d="M2 12s3.6-6.2 10-6.2S22 12 22 12s-3.6 6.2-10 6.2S2 12 2 12z"/><circle cx="12" cy="12" r="3"/></symbol>
<symbol id="hud-expand" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" d="M3.5 9V3.5H9M15 3.5h5.5V9M20.5 15v5.5H15M9 20.5H3.5V15"/></symbol>
<symbol id="hud-collapse" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" d="M9 3.5V9H3.5M20.5 9H15V3.5M15 20.5V15h5.5M3.5 15H9v5.5"/></symbol>
<symbol id="hud-pause" viewBox="0 0 24 24"><path d="M6.5 4h4v16h-4zM13.5 4h4v16h-4z"/></symbol>
<symbol id="hud-target" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-width="2.2" d="M12 4.5a7.5 7.5 0 1 1 0 15 7.5 7.5 0 0 1 0-15zM12 1v6M12 17v6M1 12h6M17 12h6"/></symbol>
<symbol id="hud-shield" viewBox="0 0 24 24"><path d="M12 1.8 21 5v6.4c0 5.4-3.7 9.4-9 11-5.3-1.6-9-5.6-9-11V5z"/></symbol>
<symbol id="hud-hull" viewBox="0 0 24 24"><path d="M9.2 2.5h5.6a1 1 0 0 1 1 1v5.7h5.7a1 1 0 0 1 1 1v5.6a1 1 0 0 1-1 1h-5.7v5.7a1 1 0 0 1-1 1H9.2a1 1 0 0 1-1-1v-5.7H2.5a1 1 0 0 1-1-1v-5.6a1 1 0 0 1 1-1h5.7V3.5a1 1 0 0 1 1-1z"/></symbol>
<symbol id="hud-rapid" viewBox="0 0 24 24"><path d="M2.5 5h4.2l6 7-6 7H2.5l6-7zm8.5 0h4.2l6 7-6 7H11l6-7z"/></symbol>
<symbol id="hud-boost" viewBox="0 0 24 24"><path d="M14.2 1.5 4 13.6h6.4L8.8 22.5 20 9.8h-6.6z"/></symbol>
<symbol id="hud-repair" viewBox="0 0 24 24"><path d="M15.6 2.2a6 6 0 0 0-5.9 7.6L2.6 16.9a2.2 2.2 0 0 0 3.1 3.1l7.1-7.1a6 6 0 0 0 7.6-5.9l-3.4 3.4-3.1-.9-.9-3.1z"/></symbol>
<symbol id="hud-lock" viewBox="0 0 24 24"><path fill-rule="evenodd" d="M12 2.5a5 5 0 0 1 5 5V10h1.2c.9 0 1.6.7 1.6 1.6v8.3c0 .9-.7 1.6-1.6 1.6H5.8c-.9 0-1.6-.7-1.6-1.6v-8.3c0-.9.7-1.6 1.6-1.6H7V7.5a5 5 0 0 1 5-5zm0 2.6a2.4 2.4 0 0 0-2.4 2.4V10h4.8V7.5A2.4 2.4 0 0 0 12 5.1zm0 8.2a1.7 1.7 0 0 0-.9 3.1v2h1.8v-2a1.7 1.7 0 0 0-.9-3.1z"/></symbol>
<symbol id="hud-skull" viewBox="0 0 24 24"><path fill-rule="evenodd" d="M12 2c5 0 8.5 3.4 8.5 8.1 0 2.6-1.1 4.5-3 5.6V19a1.5 1.5 0 0 1-1.5 1.5h-1.2V18h-1.6v2.5h-2.4V18H9.2v2.5H8A1.5 1.5 0 0 1 6.5 19v-3.3c-1.9-1.1-3-3-3-5.6C3.5 5.4 7 2 12 2zM8.6 9.2a2 2 0 1 0 0 4 2 2 0 0 0 0-4zm6.8 0a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM12 13.4l-1.3 2.3h2.6z"/></symbol>
</svg>`;

/** The kill-feed and notice icon for each way to destroy a tank. */
export const CAUSE_ICONS: Record<string, string> = {
  standard: "hud-standard",
  spread: "hud-spread",
  rocket: "hud-rocket",
  tow: "hud-rocket",
  ricochet: "hud-ricochet",
  piercing: "hud-piercing",
  mine: "hud-mine",
  drum: "hud-drum",
  interception: "hud-blast",
  explosion: "hud-blast",
};

/** An SVG reference to one of `HUD_ICONS`, for markup strings. */
export function hudIcon(id: string, className = "hud-icon"): string {
  return `<svg class="${className}" aria-hidden="true" focusable="false"><use href="#${id}" /></svg>`;
}

/** The segmented hull bar: a fill in the hull's state colour over a ghost that trails
 * each hit (`showTankStatus` keeps both). */
const HULL_BAR = `<div class="hull-bar" data-tone="healthy"><i class="hull-ghost"></i><i class="hull-fill"></i></div>`;

export function hudMarkup(): string {
  return `<div id="hud">${HUD_ICONS}
        <div id="damage-vignette" aria-hidden="true"></div>
        <div id="cockpit" aria-hidden="true"><p class="aim-hint" hidden>Click to aim · Esc for menu</p></div>
        <div class="scoreboard"><div class="team mint"><small id="label0">BLUE</small><b id="score0">0</b><i class="team-progress"><i id="progress0"></i></i></div>
        <div class="clock"><b id="time">5:00</b><small id="objective"></small></div>
        <div class="team coral"><small id="label1">RED</small><b id="score1">0</b><i class="team-progress"><i id="progress1"></i></i></div><b class="your-side" aria-hidden="true">YOUR TEAM</b></div>
        <div class="hud-actions"><button id="view-mode" class="quiet" type="button" aria-label="First-person view" title="First-person view (V)" aria-pressed="false">${hudIcon("hud-eye")}</button><button id="fullscreen" class="quiet" type="button" aria-label="Enter fullscreen" title="Enter fullscreen" aria-pressed="false">${hudIcon("hud-expand")}</button><button id="pause" class="quiet" type="button" aria-label="Pause" title="Pause (Esc)">${hudIcon("hud-pause")}</button>${SETTINGS_BUTTON}</div>
        <div id="feed"></div><div id="toast"></div><div id="damage-direction" role="img" aria-label="Incoming damage" hidden><i></i></div>
        <div class="bottom"><div class="combat-status"><div class="status"><header class="tank-label"><small id="vehicle-name">BRUISER</small><b id="rank">ROOKIE</b></header><div class="hull-readout" data-tone="healthy" title="Hull">${hudIcon("hud-hull", "hull-icon")}<b id="hp">100</b><span class="hull-max" id="hp-max">/ 100</span></div>${HULL_BAR}</div>
        <div class="weapon"><div class="ammo-strip" role="group" aria-label="Ammunition">${AMMO_OPTIONS.map(
          ({ weapon, label, name, color, help }, index) =>
            `<button type="button" class="ammo-slot" id="ammo-${weapon}" data-ammo="${weapon}" aria-pressed="false" aria-description="${help}" title="${index + 1}: ${name} — ${help}" style="--ammo-color:#${color.toString(16).padStart(6, "0")}"><kbd>${index + 1}</kbd>${hudIcon(`hud-${weapon}`, "ammo-icon")}${hudIcon("hud-lock", "ammo-lock")}<small>${label}</small><b id="ammo-count-${weapon}">${weapon === "standard" ? "∞" : "0"}</b></button>`,
        ).join(
          "",
        )}</div></div><div class="mine-slot" id="mine-slot" title="Mine (right click)"><span class="mine-dial">${hudIcon("hud-mine", "mine-icon")}<b id="mine-time"></b></span><kbd>RMB</kbd><span class="sr-only" id="mine">MINE READY</span></div><div id="effects" class="effects"></div></div>
        <div class="hull-compass" aria-hidden="true" title="First person: V returns to the overhead view"><span class="compass-dial"><i class="hull"></i><i class="gun"></i></span><kbd class="compass-key">V</kbd></div>
        <div class="combat-notices"><div id="ammo-notice" role="status" aria-live="polite"></div></div>
        </div></div>
        <div id="respawn-layer" inert></div>
        <div id="overlay"></div>`;
}

/** One line of the controls card: what it does and the keys or touches that do it. */
interface ControlLine {
  action: string;
  keys: string[];
  note?: string;
}

/** Keyboard and mouse, as the pause menu and the room menu list them. */
const KEYBOARD_CONTROLS: ControlLine[] = [
  { action: "Drive", keys: ["W", "A", "S", "D"] },
  { action: "Aim", keys: ["Mouse"] },
  { action: "Fire", keys: ["Left click"], note: "hold" },
  { action: "Drop mine", keys: ["Right click"] },
  { action: "Ammo", keys: ["Q", "E", "1–5"], note: "or wheel" },
  { action: "Zoom", keys: ["Shift", "Wheel"] },
  { action: "First person", keys: ["V"], note: "turret follows mouse" },
  { action: "Pause", keys: ["Esc"] },
];

/** The same moves on a touchscreen. */
const TOUCH_CONTROLS: ControlLine[] = [
  { action: "Drive", keys: ["Left stick"] },
  { action: "Aim and fire", keys: ["Touch the arena"] },
  { action: "Drop mine", keys: ["Mine button"] },
  { action: "Ammo", keys: ["Tap a round"] },
  { action: "Zoom", keys: ["−", "+"] },
  { action: "First person", keys: ["Eye button"], note: "stick sideways turns" },
];

function controlRows(lines: ControlLine[]): string {
  return lines
    .map(
      ({ action, keys, note }) =>
        `<div class="control-line"><dt>${action}</dt><dd>${keys.map((key) => `<kbd>${key}</kbd>`).join("")}${note ? `<small>${note}</small>` : ""}</dd></div>`,
    )
    .join("");
}

/** The controls card: keyboard and mouse, or touch where touch controls are on (the CSS
 * shows one or the other), and the two ammunition rules worth knowing. */
export function controlsMarkup(): string {
  return `<div class="controls-card" aria-label="Controls">
    <dl class="controls-list keyboard-controls">${controlRows(KEYBOARD_CONTROLS)}</dl>
    <dl class="controls-list touch-only-controls">${controlRows(TOUCH_CONTROLS)}</dl>
    <p class="controls-note"><b>TIP</b>Crates refill special ammo. Standard shells never run out.</p>
  </div>`;
}

/** Who wrecked the viewer, for the respawn card: the killer's name in their team's
 * colour, and tags for the weapon, the tank they drive and their kills so far. */
/** 1st, 2nd, 3rd, 4th … 11th, 12th, 13th, 21st. */
export function ordinal(value: number): string {
  const tens = value % 100;
  const suffix = tens >= 11 && tens <= 13 ? "TH" : (["TH", "ST", "ND", "RD"][value % 10] ?? "TH");
  return `${value}${suffix}`;
}

export function wreckedByMarkup(death: DeathReport | null): string {
  if (!death) {
    return `<div class="wrecked-by"><small>YOUR TANK</small><b>DESTROYED</b></div>`;
  }
  const icon = death.cause ? CAUSE_ICONS[death.cause] : undefined;
  const tags = [
    death.weapon
      ? `<span class="wrecked-weapon">${icon ? hudIcon(icon, "wrecked-icon") : ""}${death.weapon}</span>`
      : "",
    death.killerVehicle && !death.self
      ? `<span class="killer-tag">${death.killerVehicle}</span>`
      : "",
    death.killerKills && !death.self
      ? `<span class="killer-tag">${hudIcon("hud-skull", "killer-tag-icon")}Their ${ordinal(death.killerKills).toLowerCase()} kill</span>`
      : "",
  ].join("");
  const [label, name] = death.self
    ? ["OWN GOAL", "YOU"]
    : death.killer
      ? ["DESTROYED BY", escapeHtml(death.killer)]
      : ["DESTROYED BY", "THE YARD"];
  const side = death.killerTeam === 0 ? " side-blue" : death.killerTeam === 1 ? " side-red" : "";
  return `<div class="wrecked-by${death.self ? " self" : ""}${side}"><small>${label}</small><b>${name}</b>${tags ? `<div class="wrecked-tags">${tags}</div>` : ""}</div>`;
}

export { escapeHtml };

/** The death the respawn card and a solo report describe: its parts and its sentence. */
export interface DeathNotice {
  report: DeathReport;
  text: string;
}

/** The live or final score on team plates named under the numbers, for pause and the
 * battle report. */
/** Both teams' scores as a pair of plates; the viewer's side carries the in-match
 * scoreboard's gold rim and says YOU beside its name. */
export function scoreMarkup(
  scores: readonly number[],
  label: string,
  names: readonly string[],
  team: number,
): string {
  const plate = (index: number, side: string) =>
    `<span class="${side}${index === team ? " yours" : ""}">${scores[index]}<small>${names[index]}${index === team ? "<em> · YOU</em>" : ""}</small></span>`;
  return `<div class="dialog-score" aria-label="${label}">${plate(0, "blue")}<i>:</i>${plate(1, "red")}</div>`;
}

/** How a team battle came out for the viewer's team. A battle ended early goes to the
 * side ahead when it ended; level scores are a draw. */
export function teamOutcome(
  scores: readonly number[],
  team: number,
  winner: number | null,
  endedEarly: boolean,
): "won" | "lost" | "draw" {
  const leader = endedEarly || winner === null ? leadingTeam(scores) : winner;
  return leader === null ? "draw" : leader === team ? "won" : "lost";
}

function leadingTeam(scores: readonly number[]): number | null {
  return scores[0] === scores[1] ? null : scores[0] > scores[1] ? 0 : 1;
}

/** Small marks leading the dialogs' buttons, drawn in the button's text colour. */
const actionIcon = (path: string) =>
  `<svg class="action-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="${path}"/></svg>`;
const ACTION_ICONS = {
  resume: actionIcon("M7 4.5v15l12.5-7.5z"),
  settings: actionIcon(
    "M4 6.5h9.2a2.8 2.8 0 0 1 5.6 0H20v2h-1.2a2.8 2.8 0 0 1-5.6 0H4zm0 9h1.2a2.8 2.8 0 0 1 5.6 0H20v2h-9.2a2.8 2.8 0 0 1-5.6 0H4z",
  ),
  end: actionIcon("M5 3.5h2v17H5zM8.5 4h10l-2.5 4 2.5 4h-10z"),
  again: actionIcon(
    "M12 4.5a7.5 7.5 0 1 1-7.1 9.9l2.1-.7A5.3 5.3 0 1 0 8.4 8L11 10.5H4V3.5l2.9 2.9A7.5 7.5 0 0 1 12 4.5z",
  ),
  setup: actionIcon("M4 4h7v7H4zm9 0h7v7h-7zM4 13h7v7H4zm9 0h7v7h-7z"),
};

/** Pause's first tab: where the battle stands, in the engine's words. A solo run's tally,
 * then the clock and the viewer's tank and round so far. A team battle's score is on the
 * header's plates already; the body adds only facts. */
function pauseOverviewMarkup(pause: PauseReport): string {
  const { tally } = pause;
  const objective = tally
    ? `<div class="pause-objective solo"><div><b>${tally.kills}</b><small>KILLS</small></div><div><b>${tally.survived}</b><small>SURVIVED</small></div></div>`
    : "";
  const fact = ({ label, value, note }: PauseReport["facts"][number]) =>
    `<div><dt>${label}</dt><dd>${escapeHtml(value)}${note ? `<small>${escapeHtml(note)}</small>` : ""}</dd></div>`;
  return `${objective}
    <dl class="pause-facts">${pause.facts.map(fact).join("")}</dl>`;
}

/** The menu for the battle's phase: pause (with `pause`, the engine's `pause_json`), the
 * battle report, or the respawn card. */
export function menuMarkup(
  hud: HudState,
  death: DeathNotice | null,
  pause: PauseReport | null = null,
): string {
  const viewer = hud.human;
  const phase = hud.match.phase;
  const solo = hud.gameMode === "solo";
  const mode = solo ? "SOLO ASSAULT" : "TEAM BATTLE";
  if (phase === "paused") {
    const standing = pause?.standing;
    return `
      <section class="menu compact pause-menu" aria-labelledby="pause-title">
        <button id="pause-close" class="dialog-close" type="button" aria-label="Resume the battle" title="Resume (Esc)">${CLOSE_ICON}</button>
        <header class="dialog-head"><div class="dialog-eyebrow"><span class="eyebrow">${mode} · ${hud.mapName.toUpperCase()}</span></div><div class="dialog-title"><h2 id="pause-title">PAUSED</h2>${solo ? "" : scoreMarkup(hud.match.scores, "Score", hud.teamNames, hud.humanTeam)}</div>${solo ? `<p class="dialog-lede">The battle waits until you resume.</p>` : standing ? `<p class="dialog-standing" data-tone="${standing.tone}">${standing.text}</p>` : ""}</header>
        <div class="dialog-tabs" role="tablist" aria-label="Pause"><button type="button" role="tab" id="pause-tab-battle" aria-selected="true" aria-controls="pause-battle">BATTLE</button><button type="button" role="tab" id="pause-tab-controls" aria-selected="false" aria-controls="pause-controls" tabindex="-1">CONTROLS</button></div>
        <div class="dialog-pane" id="pause-battle" role="tabpanel" aria-labelledby="pause-tab-battle">${pause ? pauseOverviewMarkup(pause) : ""}</div>
        <div class="dialog-pane" id="pause-controls" role="tabpanel" aria-labelledby="pause-tab-controls" hidden>${controlsMarkup()}</div>
        <div class="menu-actions pause-actions"><button id="resume" class="primary">${ACTION_ICONS.resume}RESUME</button>
        <button id="pause-settings" class="secondary" type="button" aria-haspopup="dialog">${ACTION_ICONS.settings}SETTINGS</button>
        <button id="end-battle" class="secondary danger" title="End the battle and see the report">${ACTION_ICONS.end}END BATTLE</button></div>
        </section>`;
  } else if (phase === "results") {
    // The engine words the outcome, the report and the board's order once results show.
    const recap = hud.recap;
    if (!recap) {
      return "";
    }
    const { report } = recap;
    const endedEarly = !!hud.match.endedEarly;
    const scores = hud.match.scores;
    const team = hud.humanTeam;
    const side = team === 0 ? "blue" : "red";
    // A solo run's loss leads with the death that ended it.
    const lede =
      solo && report.outcome === "lost" && death
        ? `${escapeHtml(death.text)} ${report.lede}`
        : report.lede;
    return `<section class="menu compact results" data-outcome="${report.outcome}" data-team="${side}" aria-labelledby="results-title">
      <header class="dialog-head results-banner"><div class="dialog-eyebrow"><span class="eyebrow">${mode} · ${hud.mapName.toUpperCase()}</span>${endedEarly && !solo ? `<span class="early-tag">ENDED EARLY</span>` : ""}</div>
      <div class="dialog-title"><h2 id="results-title">${report.title}</h2>${solo ? "" : scoreMarkup(scores, "Final score", hud.teamNames, team)}</div>
      <p class="dialog-lede">${lede}</p></header>
      <div class="results-body${solo ? "" : " with-board"}"><div class="results-you">${recapMarkup(recap)}</div>${solo ? "" : scoreboardMarkup(hud.scoreboard, report.board, viewer.id, hud.teamNames, scores)}</div>
      <div class="menu-actions"><button id="play-again" class="primary">${ACTION_ICONS.again}PLAY AGAIN</button>
      <button id="restart" class="secondary">${ACTION_ICONS.setup}BATTLE SETUP</button></div>
    </section>`;
  } else if (!viewer.alive) {
    return respawnMarkup(death);
  }
  return "";
}

/** The respawn card: who wrecked the tank, the countdown and the choice of the next
 * tank (whose cards the page appends). */
export function respawnMarkup(death: DeathNotice | null): string {
  return `
      <section class="menu respawn" aria-labelledby="respawn-title">
        <header class="respawn-head">${wreckedByMarkup(death?.report ?? null)}
          <div class="respawn-timer" id="respawn-fill"><h2 id="respawn-title">RESPAWN IN</h2><div class="respawn-ring"><span id="respawn-count">3</span></div></div>
        </header>
        <p class="respawn-pick">Choose your next tank<small>It rolls out when the timer ends</small></p>
        <p id="death-cause" class="sr-only" role="status"></p>
        </section>`;
}
