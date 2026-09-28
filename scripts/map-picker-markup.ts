import { MAP_OPTIONS, type MapOption } from "../src/game/map-options.ts";

function mapIcon(map: MapOption): string {
  return `<svg class="choice-icon" style="--tint: ${map.tint}" aria-hidden="true"><use href="#icon-${map.icon}" /></svg>`;
}

function mapLabel(map: MapOption): string {
  const badge = "extra" in map ? '<em class="level-badge">EXTRA</em>' : "";
  return (
    mapIcon(map) +
    `<span class="map-picker-text"><b>${map.name}${badge}</b><small>${map.description}</small></span>`
  );
}

/** The standard maps as a row of radio buttons: full cards, or compact `tiles` for the
 * new room's narrow column. */
function mapRowMarkup(name: string, labelledBy: string, tiles: boolean): string {
  const standard = MAP_OPTIONS.filter((map) => !("extra" in map));
  const item = (map: MapOption, index: number) =>
    `<label class="${tiles ? "room-map" : "choice-card"}"><input type="radio" name="${name}" value="${map.id}"${index === 0 ? " checked" : ""} />` +
    mapIcon(map) +
    (tiles
      ? `<span>${map.name}</span>`
      : `<span><b>${map.name}</b><small>${map.description}</small></span>`) +
    `</label>`;
  return (
    `<div class="map-row ${tiles ? "room-maps" : "choice-cards"}" role="radiogroup" aria-labelledby="${labelledBy}">` +
    standard.map(item).join("") +
    `</div>`
  );
}

/** A map dropdown. Its extra levels sit in their own group; the whole dropdown stays
 * hidden, in favour of the row, until `showExtraLevels`. */
function mapPickerMarkup(name: string, labelledBy: string): string {
  const option = (map: MapOption, selected: boolean) =>
    `<div id="${name}-${map.id}" class="map-picker-option" role="option" data-value="${map.id}" aria-selected="${selected}">${mapLabel(map)}</div>`;
  const standard = MAP_OPTIONS.filter((map) => !("extra" in map));
  const extra = MAP_OPTIONS.filter((map) => "extra" in map);
  return (
    `<div class="map-picker" data-name="${name}" data-value="${standard[0].id}" hidden>` +
    `<div class="map-picker-button" role="combobox" tabindex="0" aria-haspopup="listbox" aria-expanded="false" aria-controls="${name}-list" aria-labelledby="${labelledBy}">` +
    `<span class="map-picker-current">${mapLabel(standard[0])}</span>` +
    `<svg class="map-picker-chevron" viewBox="0 0 24 24" aria-hidden="true"><path d="m7 10 5 5 5-5" /></svg></div>` +
    `<div id="${name}-list" class="map-picker-list" role="listbox" aria-labelledby="${labelledBy}" popover="manual">` +
    standard.map((map, index) => option(map, index === 0)).join("") +
    `<div class="map-picker-group" role="group" aria-labelledby="${name}-extra" hidden>` +
    `<div id="${name}-extra" class="map-picker-group-label">Extra levels</div>` +
    extra.map((map) => option(map, false)).join("") +
    `</div></div></div>`
  );
}

/** One map choice, built into Battle Setup's markup at build time so the menu paints
 * complete; `src/game/map-picker.ts` keeps its row and dropdown in step. `name` is the
 * choice it edits and `labelledBy` its section label's id. */
export function mapChoiceMarkup(name: string, labelledBy: string, tiles: boolean): string {
  return mapRowMarkup(name, labelledBy, tiles) + mapPickerMarkup(name, labelledBy);
}
