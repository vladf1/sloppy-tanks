import { DIFFICULTIES } from "./difficulty";
import { SCORE_LIMIT, VEHICLES } from "./data";
import { tankPreview } from "./tank-previews";
import { MAP_OPTIONS } from "./map-options";
import type { VehicleKind } from "./types";
import type { GameOptions, MenuOptions } from "./game-options";

export const CONTROL_HELP =
  "WASD / Arrows: steer toward direction · Opposite direction: reverse · Mouse: aim · Hold left click: fire · Right click: mine · Esc: pause<br>Q / E / scroll: cycle ammo · 1–5: select · Shift + scroll: zoom · Crates refill ammo · Standard shells: unlimited";

export function vehicleCards(simulation: GameOptions): string {
  return `<div class="vehicles">${(Object.keys(VEHICLES) as VehicleKind[])
    .map((kind) => {
      const v = VEHICLES[kind];
      return `
          <button class="vehicle ${simulation.humanKind === kind ? "selected" : ""}" data-kind="${kind}" aria-pressed="${simulation.humanKind === kind}">
            <strong>${v.name}</strong><small>${v.tag}</small>
            ${tankPreview(kind, simulation.humanTeam, `${v.name} tank`)}
            <div class="spec"><span>${v.health} HIT POINTS</span><span>${v.speedKmh} KM/H</span></div>
          </button>`;
    })
    .join("")}</div>`;
}

function modeOptions(simulation: MenuOptions): string {
  if (simulation.customMap) {
    return `<div class="mode-options"><fieldset><legend>BATTLE</legend><div class="mode-option fixed"><span><b>30-Tank Stress Battle</b><small>15 vs 15 · Endless respawns and scoring · No victory</small></span></div></fieldset><fieldset><legend>MAP</legend><div class="mode-option fixed"><span><b>${simulation.customMap.name}</b><small>${simulation.customMap.description}</small></span></div></fieldset></div>`;
  }
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
    ...MAP_OPTIONS.map((map) => [map.id, map.name, map.description]),
    ["surprise", "Surprise me", "Pick my next battleground."],
  ])}</div>`;
}

function difficultyOptions(simulation: MenuOptions): string {
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

export function startMenuMarkup(simulation: MenuOptions): string {
  return `
      <section class="menu start">
        <div class="start-heading"><h1>BATTLE SETUP</h1><button id="start" class="primary" type="button" aria-label="GO! Start round">GO!</button></div>
        ${modeOptions(simulation)}
        ${difficultyOptions(simulation)}
        ${vehicleCards(simulation)}
        <div class="menu-footer"><div class="menu-help">${CONTROL_HELP}</div></div>
        </section>`;
}
