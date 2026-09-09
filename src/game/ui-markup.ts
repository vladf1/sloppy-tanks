import { AMMO_HELP, AMMO_ORDER } from "./ammunition";
import { DIFFICULTIES } from "./difficulty";
import { SCORE_LIMIT, TEAM_NAMES, VEHICLES, WEAPONS } from "./data";
import type { Simulation } from "./simulation";
import { speedTuning } from "./speed-tuning";
import { tankPreview } from "./tank-previews";
import type { VehicleKind } from "./types";

const CONTROL_HELP =
  "WASD / Arrows: steer toward direction · Opposite direction: reverse · Mouse: aim · Hold left click: fire · Right click: mine · Esc: pause<br>Q / E / scroll: cycle ammo · 1–5: select · Shift + scroll: zoom · Crates refill ammo · Standard shells: unlimited";

export function hudMarkup(): string {
  return `<div id="hud">
        <div class="brand">SLOPPY<span>TANKS</span></div>
        <div class="scoreboard"><div class="team mint"><small id="label0">◆ BLUE</small><b id="score0">0</b></div>
        <div class="clock"><b id="time">5:00</b><small id="objective">FIRST TO ${SCORE_LIMIT}</small></div>
        <div class="team coral"><small id="label1">RED Ⅱ</small><b id="score1">0</b></div></div>
        <button id="pause" class="quiet">Ⅱ <span>PAUSE</span></button>
        <div id="feed"></div><div id="toast"></div><div id="damage-direction" role="img" aria-label="Incoming damage" hidden><i></i></div>
        <div class="bottom"><div class="combat-status"><div class="status"><header class="tank-label"><small id="vehicle-name">BRUISER</small><b id="rank">ROOKIE</b></header><div><b id="hp">100</b><span>HULL</span><i id="hpbar"></i></div></div>
        <div class="weapon"><div class="ammo-strip" role="group" aria-label="Ammunition">${AMMO_ORDER.map(
          (weapon, index) =>
            `<button type="button" class="ammo-slot" id="ammo-${weapon}" data-ammo="${weapon}" aria-pressed="false" aria-description="${AMMO_HELP[weapon]}" title="${index + 1}: ${WEAPONS[weapon].name} — ${AMMO_HELP[weapon]}" style="--ammo-color:#${WEAPONS[weapon].color.toString(16).padStart(6, "0")}"><kbd>${index + 1}</kbd><small>${WEAPONS[weapon].label}</small><b id="ammo-count-${weapon}">${weapon === "standard" ? "∞" : "0"}</b></button>`,
        ).join("")}</div><span id="mine">MINE READY · RMB</span></div></div>
        <div class="combat-notices"><div id="ammo-notice" role="status" aria-live="polite"></div><em id="effects"></em></div>
        </div></div>
        <div id="overlay"></div>`;
}

export function vehicleCards(simulation: Simulation): string {
  return `<div class="vehicles">${(Object.keys(VEHICLES) as VehicleKind[])
    .map((kind) => {
      const v = VEHICLES[kind];
      return `
          <button class="vehicle ${simulation.humanKind === kind ? "selected" : ""}" data-kind="${kind}">
            <strong>${v.name}</strong><small>${v.tag}</small>
            <img class="tank-preview" src="${tankPreview(kind, simulation.humanTeam)}" alt="${v.name} tank" draggable="false">
            <div class="spec"><span>${v.health} HIT POINTS</span><span>${v.speedKmh} KM/H</span></div>
          </button>`;
    })
    .join("")}</div>`;
}

export function modeOptions(simulation: Simulation): string {
  const group = (key: "gameMode" | "mapMode", title: string, options: string[][]) =>
    `<fieldset><legend>${title}</legend>${options
      .map(
        ([value, title, detail]) =>
          `<label class="mode-option"><input type="radio" name="${key}" value="${value}" ${simulation[key] === value ? "checked" : ""}><span><b>${title}</b><small>${detail}</small></span></label>`,
      )
      .join("")}</fieldset>`;
  return `<div class="mode-options">${group("gameMode", "BATTLE", [
    ["team", "Team Battle", `6 vs 6 · Respawns · First to ${SCORE_LIMIT}`],
    ["solo", "Solo Assault", "Endless enemies · 10 minutes · One life"],
  ])}${group("mapMode", "MAP", [
    ["village", "Pine Village", "The original arena"],
    ["random", "Random Map", "Fresh layout every round"],
  ])}</div>`;
}

export function difficultyOptions(simulation: Simulation): string {
  return `<fieldset class="difficulty-setting" aria-describedby="difficulty-help"><legend>DIFFICULTY</legend><div class="difficulty-options">${Object.entries(
    DIFFICULTIES,
  )
    .map(
      ([key, value]) =>
        `<label class="difficulty-option"><input type="radio" name="difficulty" value="${key}" ${simulation.difficulty === key ? "checked" : ""}><span>${value.label}</span></label>`,
    )
    .join(
      "",
    )}</div><small id="difficulty-help">${DIFFICULTIES[simulation.difficulty].description}</small></fieldset>`;
}

export function speedSliders(): string {
  return `<div class="speed-tuning">${(["tank-speed", "bullet-speed"] as const)
    .map(
      (key) =>
        `<label for="${key}"><span>${key === "tank-speed" ? "Tank base speed" : "Projectile base speed"} <output id="${key}-value" for="${key}">${Math.round(speedTuning[key] * 100)}%</output></span><input id="${key}" type="range" min="0.5" max="2" step="0.05" value="${speedTuning[key]}"></label>`,
    )
    .join("")}<small>50–200% · 100% = default speed · Saved automatically</small></div>`;
}

export function menuMarkup(simulation: Simulation): string {
  const phase = simulation.match.phase;
  if (phase === "ready") {
    return `
      <section class="menu start">
        <div class="eyebrow">${simulation.mapName} / ${simulation.gameMode === "solo" ? "SURVIVAL" : "6 V 6"}</div>
        <h1>CHOOSE YOUR TANK</h1>
        <p class="intro">Choose your battle, then click a tank to start.</p>
        ${modeOptions(simulation)}
        ${difficultyOptions(simulation)}
        ${vehicleCards(simulation)}
        <div class="menu-foot"><div><b>${simulation.gameMode === "solo" ? "YOUR TANK" : "YOUR TEAM"}: ${simulation.humanTeam === 0 ? "◆" : "Ⅱ"} ${TEAM_NAMES[simulation.humanTeam]}</b><small>${simulation.gameMode === "solo" ? "10 MINUTES · ENDLESS ENEMIES · ONE LIFE" : `5 MINUTES · FIRST TO ${SCORE_LIMIT} · FRIENDLY FIRE OFF`}</small></div></div>
        <div class="menu-help">${CONTROL_HELP}</div>
        </section>`;
  } else if (phase === "paused") {
    return `
      <section class="menu compact">
        <h2>PAUSED</h2>
        <p>${CONTROL_HELP}</p>
        <label>Sound <input id="volume" type="range" min="0" max="1" step=".05" value="${localStorage.getItem("sloppy-volume") ?? ".6"}"></label>
        ${speedSliders()}
        <button id="resume" class="primary">RESUME</button>
        <button id="restart" class="secondary">New round / choose vehicle</button>
        </section>`;
  } else if (phase === "results" && simulation.gameMode === "solo") {
    return `
      <section class="menu compact">
        <div class="eyebrow">SOLO ASSAULT / ${simulation.mapName}</div>
        <h2>${simulation.match.winner === simulation.humanTeam ? "SURVIVED" : "TANK DESTROYED"}</h2>
        <div class="result-score">${simulation.human.kills}</div>
        <p>Enemy kills.<br>${!simulation.human.alive ? "Your run is over." : "You survived the full ten minutes."}</p>
        <p id="death-cause" role="status"></p>
        <button id="restart" class="primary">ANOTHER ROUND</button>
        </section>`;
  } else if (phase === "results") {
    return `
      <section class="menu compact">
        <div class="eyebrow">ROUND COMPLETE / ${simulation.mapName}</div>
        <h2>${simulation.match.winner === simulation.humanTeam ? "VICTORY" : "DEFEAT"}</h2>
        <div class="result-score"><span>${simulation.match.scores[0]}</span> : <span>${simulation.match.scores[1]}</span></div>
        <p>${TEAM_NAMES[simulation.match.winner ?? 0]} wins${simulation.match.overtime ? " in overtime" : ""}.<br>You scored ${simulation.human.kills} eliminations · ${simulation.human.deaths} wrecks<br>${simulation.destroyed} pieces of cover demolished.</p>
        <p id="death-cause" role="status"></p>
        <button id="restart" class="primary">ANOTHER ROUND</button>
        </section>`;
  } else if (!simulation.human.alive) {
    return `
      <section class="menu respawn">
        <h2>Respawn in <span id="respawn-count">3</span></h2>
        <p id="death-cause" role="status"></p>
        ${vehicleCards(simulation)}
        </section>`;
  }
  return "";
}
