import { recapMarkup, teamLabel, wreckLine } from "./round-recap";
import { AMMO_OPTIONS } from "./ammo-options";
import type { HudState } from "./engine-api";
import type { DeathReport } from "./hud-feedback";
import { SETTINGS_BUTTON } from "./settings-dialog";

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
        <div id="cockpit" aria-hidden="true"><div class="periscope-frame"><i></i><i></i><i></i><i></i></div><div class="hull-compass"><i class="hull"></i><i class="gun"></i></div><p class="aim-hint" hidden>Click to aim · Esc for menu</p></div>
        <div class="scoreboard"><div class="team mint"><small id="label0">BLUE</small><b id="score0">0</b><i class="team-progress"><i id="progress0"></i></i></div>
        <div class="clock"><b id="time">5:00</b><small id="objective"></small></div>
        <div class="team coral"><small id="label1">RED</small><b id="score1">0</b><i class="team-progress"><i id="progress1"></i></i></div></div>
        <div class="hud-actions"><button id="view-mode" class="quiet" type="button" aria-label="First-person view" title="First-person view (V)" aria-pressed="false">${hudIcon("hud-eye")}</button><button id="fullscreen" class="quiet" type="button" aria-label="Enter fullscreen" title="Enter fullscreen" aria-pressed="false">${hudIcon("hud-expand")}</button><button id="pause" class="quiet" type="button" aria-label="Pause" title="Pause (Esc)">${hudIcon("hud-pause")}</button>${SETTINGS_BUTTON}</div>
        <div id="feed"></div><div id="toast"></div><div id="damage-direction" role="img" aria-label="Incoming damage" hidden><i></i></div>
        <div id="hit-marker" aria-hidden="true"><i></i><i></i><i></i><i></i></div>
        <div id="kill-banner" aria-hidden="true"><i class="kill-flare"></i>${hudIcon("hud-skull", "kill-icon")}<span><small>ELIMINATED</small><b></b></span></div>
        <div id="hull-pill" aria-hidden="true">${hudIcon("hud-shield", "hull-pill-icon")}<b id="hull-pill-hp">100</b>${HULL_BAR}</div>
        <div class="bottom"><div class="combat-status"><div class="status"><header class="tank-label"><small id="vehicle-name">BRUISER</small><b id="rank">ROOKIE</b></header><div class="hull-readout" data-tone="healthy"><b id="hp">100</b><span class="hull-max" id="hp-max">/ 100</span><span>HULL</span></div>${HULL_BAR}</div>
        <div class="weapon"><div class="ammo-strip" role="group" aria-label="Ammunition">${AMMO_OPTIONS.map(
          ({ weapon, label, name, color, help }, index) =>
            `<button type="button" class="ammo-slot" id="ammo-${weapon}" data-ammo="${weapon}" aria-pressed="false" aria-description="${help}" title="${index + 1}: ${name} — ${help}" style="--ammo-color:#${color.toString(16).padStart(6, "0")}"><kbd>${index + 1}</kbd>${hudIcon(`hud-${weapon}`, "ammo-icon")}<small>${label}</small><b id="ammo-count-${weapon}">${weapon === "standard" ? "∞" : "0"}</b></button>`,
        ).join(
          "",
        )}</div><div class="panel-foot"><div class="mine-line">${hudIcon("hud-mine", "mine-icon")}<span id="mine">MINE READY · RMB</span></div><div id="effects" class="effects"></div></div></div></div>
        <div class="combat-notices"><div id="ammo-notice" role="status" aria-live="polite"></div></div>
        </div></div>
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

/** Who wrecked the viewer, for the respawn card. */
function wreckedByMarkup(death: DeathReport | null): string {
  if (!death) {
    return `<div class="wrecked-by"><small>TANK DESTROYED</small><b>WRECKED</b></div>`;
  }
  const icon = death.cause ? CAUSE_ICONS[death.cause] : undefined;
  const weapon = death.weapon
    ? `<span class="wrecked-weapon">${icon ? hudIcon(icon, "wrecked-icon") : ""}${death.weapon}</span>`
    : "";
  const [label, name] = death.self
    ? ["OWN GOAL", "YOU"]
    : death.killer
      ? ["WRECKED BY", escapeHtml(death.killer)]
      : ["WRECKED BY", "THE YARD"];
  return `<div class="wrecked-by${death.self ? " self" : ""}"><small>${label}</small><b>${name}</b>${weapon}</div>`;
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

/** The death the respawn card and a solo report describe: its parts and its sentence. */
export interface DeathNotice {
  report: DeathReport;
  text: string;
}

/** The live or final score on team plates, for pause and the battle report. */
function scoreMarkup(scores: readonly number[], label: string): string {
  return `<div class="dialog-score" aria-label="${label}"><span class="blue">${scores[0]}</span><i>:</i><span class="red">${scores[1]}</span></div>`;
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

const OUTCOME_TITLES = { won: "VICTORY", lost: "DEFEAT", draw: "DRAW" } as const;

export function menuMarkup(hud: HudState, death: DeathNotice | null): string {
  const viewer = hud.human;
  const phase = hud.match.phase;
  const solo = hud.gameMode === "solo";
  const mode = solo ? "SOLO ASSAULT" : "TEAM BATTLE";
  if (phase === "paused") {
    return `
      <section class="menu compact pause-menu" aria-labelledby="pause-title">
        <header class="dialog-head"><div class="dialog-eyebrow"><span class="eyebrow">${mode} · ${hud.mapName.toUpperCase()}</span></div><div class="dialog-title"><h2 id="pause-title">PAUSED</h2>${solo ? "" : scoreMarkup(hud.match.scores, "Score")}</div><p class="dialog-lede">The battle waits until you resume.</p></header>
        ${controlsMarkup()}
        <div class="menu-actions"><button id="resume" class="primary">RESUME</button>
        <button id="pause-settings" class="secondary" type="button" aria-haspopup="dialog">SETTINGS</button>
        <button id="end-battle" class="secondary danger">END BATTLE</button></div>
        </section>`;
  } else if (phase === "results") {
    const endedEarly = !!hud.match.endedEarly;
    const scores = hud.match.scores;
    const team = hud.humanTeam;
    const side = team === 0 ? "blue" : "red";
    let outcome: string;
    let title: string;
    let lede: string;
    if (solo) {
      const survived = hud.match.winner === team;
      outcome = endedEarly ? "ended" : survived ? "won" : "lost";
      title = endedEarly ? "BATTLE ENDED" : survived ? "SURVIVED" : "TANK DESTROYED";
      lede = endedEarly
        ? "Run ended early."
        : survived
          ? "Ten minutes. One tank. Still standing."
          : `${death ? escapeHtml(death.text) + " " : ""}One more run?`;
    } else {
      outcome = teamOutcome(scores, team, hud.match.winner, endedEarly);
      title = OUTCOME_TITLES[outcome as keyof typeof OUTCOME_TITLES];
      const leader = leadingTeam(scores);
      const high = Math.max(scores[0], scores[1]);
      const low = Math.min(scores[0], scores[1]);
      const result = endedEarly
        ? leader === null
          ? `Level at ${high}–${low}`
          : `${teamLabel(hud.teamNames[leader])} led ${high}–${low}`
        : `${teamLabel(hud.teamNames[hud.match.winner ?? 0])} wins${hud.match.overtime ? " in overtime" : ""}`;
      lede = `${result} · ${wreckLine(viewer.deaths)}`;
    }
    return `<section class="menu compact results" data-outcome="${outcome}" data-team="${side}" aria-labelledby="results-title">
      <header class="dialog-head results-banner"><div class="dialog-eyebrow"><span class="eyebrow">${endedEarly ? "ENDED EARLY" : solo ? mode : "ROUND COMPLETE"} · ${hud.mapName.toUpperCase()}</span></div>
      <div class="dialog-title"><h2 id="results-title">${title}</h2>${solo ? "" : scoreMarkup(scores, "Final score")}</div>
      <p class="dialog-lede">${lede}</p></header>
      ${hud.recap ? recapMarkup(hud, hud.recap) : ""}
      <div class="menu-actions"><button id="play-again" class="primary">PLAY AGAIN</button>
      <button id="restart" class="secondary">BATTLE SETUP</button></div>
    </section>`;
  } else if (!viewer.alive) {
    return `
      <section class="menu respawn" aria-labelledby="respawn-title">
        <header class="respawn-head">${wreckedByMarkup(death?.report ?? null)}
          <div class="respawn-timer"><h2 id="respawn-title">RESPAWN<small>Pick your next tank</small></h2><div class="respawn-ring" id="respawn-fill"><span id="respawn-count">3</span></div></div>
        </header>
        <p id="death-cause" class="sr-only" role="status"></p>
        </section>`;
  }
  return "";
}
