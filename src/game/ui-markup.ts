import { recapMarkup } from "./round-recap";
import { AMMO_OPTIONS } from "./ammo-options";
import type { HudState } from "./engine-api";
import { SETTINGS_BUTTON } from "./settings-dialog";

export function hudMarkup(): string {
  return `<div id="hud">
        <div id="cockpit" aria-hidden="true"><div class="hull-compass"><i class="hull"></i><i class="gun"></i></div><p class="aim-hint" hidden>Click to aim · Esc for menu</p></div>
        <div class="brand">SLOPPY<span>TANKS</span></div>
        <div class="scoreboard"><div class="team mint"><small id="label0">◆ BLUE</small><b id="score0">0</b></div>
        <div class="clock"><b id="time">5:00</b><small id="objective"></small></div>
        <div class="team coral"><small id="label1">RED Ⅱ</small><b id="score1">0</b></div></div>
        <div class="hud-actions"><button id="view-mode" class="quiet" type="button" aria-label="First-person view" title="First-person view (V)" aria-pressed="false">◎</button><button id="fullscreen" class="quiet" type="button" aria-label="Enter fullscreen" title="Enter fullscreen" aria-pressed="false">⛶</button><button id="pause" class="quiet" type="button" aria-label="Pause" title="Pause">Ⅱ</button>${SETTINGS_BUTTON}</div>
        <div id="feed"></div><div id="toast"></div><div id="damage-direction" role="img" aria-label="Incoming damage" hidden><i></i></div>
        <div class="bottom"><div class="combat-status"><div class="status"><header class="tank-label"><small id="vehicle-name">BRUISER</small><b id="rank">ROOKIE</b></header><div><b id="hp">100</b><span>HULL</span><i id="hpbar"></i></div></div>
        <div class="weapon"><div class="ammo-strip" role="group" aria-label="Ammunition">${AMMO_OPTIONS.map(
          ({ weapon, label, name, color, help }, index) =>
            `<button type="button" class="ammo-slot" id="ammo-${weapon}" data-ammo="${weapon}" aria-pressed="false" aria-description="${help}" title="${index + 1}: ${name} — ${help}" style="--ammo-color:#${color.toString(16).padStart(6, "0")}"><kbd>${index + 1}</kbd><small>${label}</small><b id="ammo-count-${weapon}">${weapon === "standard" ? "∞" : "0"}</b></button>`,
        ).join("")}</div><span id="mine">MINE READY · RMB</span></div></div>
        <div class="combat-notices"><div id="ammo-notice" role="status" aria-live="polite"></div><em id="effects"></em></div>
        </div></div>
        <div id="overlay"></div>`;
}

export function menuMarkup(hud: HudState, controlHelp: string): string {
  const viewer = hud.human;
  const phase = hud.match.phase;
  if (phase === "paused") {
    return `
      <section class="menu compact" aria-labelledby="pause-title">
        <header class="dialog-head"><div class="dialog-eyebrow"><span class="eyebrow">${hud.gameMode === "solo" ? "SOLO ASSAULT" : "TEAM BATTLE"} / ${hud.mapName}</span></div><div class="dialog-title"><h2 id="pause-title">PAUSED</h2></div><p class="dialog-lede">The battle waits until you resume.</p></header>
        <p class="dialog-help">${controlHelp}</p>
        <div class="menu-actions"><button id="resume" class="primary">RESUME</button>
        <button id="end-battle" class="secondary danger">END BATTLE</button></div>
        </section>`;
  } else if (phase === "results") {
    const solo = hud.gameMode === "solo";
    const endedEarly = hud.match.endedEarly;
    const won = hud.match.winner === hud.humanTeam;
    return `<section class="menu compact results" aria-labelledby="results-title">
      <header class="dialog-head"><div class="dialog-eyebrow"><span class="eyebrow">${solo ? "SOLO ASSAULT" : "ROUND COMPLETE"} / ${hud.mapName}</span></div>
      <div class="dialog-title"><h2 id="results-title">${endedEarly ? "BATTLE ENDED" : solo ? (won ? "SURVIVED" : "TANK DESTROYED") : won ? "VICTORY" : "DEFEAT"}</h2>${solo ? "" : `<div class="dialog-score" aria-label="Final score"><span class="blue">${hud.match.scores[0]}</span> : <span class="red">${hud.match.scores[1]}</span></div>`}</div>
      <p class="dialog-lede">${solo ? (endedEarly ? "Run ended early." : won ? "Ten minutes. One tank. Still standing." : "One more run. One more personal best?") : `${endedEarly ? "Ended early" : `${hud.teamNames[hud.match.winner ?? 0]} wins${hud.match.overtime ? " in overtime" : ""}`} · ${viewer.deaths} personal wrecks`}</p></header>
      ${hud.recap ? recapMarkup(hud, hud.recap) : ""}
      <div class="menu-actions"><button id="play-again" class="primary">PLAY AGAIN</button>
      <button id="restart" class="secondary">BATTLE SETUP</button></div>
    </section>`;
  } else if (!viewer.alive) {
    return `
      <section class="menu respawn">
        <h2>Respawn in <span id="respawn-count">3</span></h2>
        <p id="death-cause" class="sr-only" role="status"></p>
        </section>`;
  }
  return "";
}
