/** The maps offered by Battle Setup, room settings and `?map=` links. This module imports
 * nothing: the inline startup script and the build's map-picker markup
 * (`scripts/map-picker-markup.ts`) both read it. */
export const MAP_OPTIONS = [
  {
    id: "village",
    name: "Pine Village",
    description: "Quiet lanes. Loud tanks.",
    icon: "village",
    tint: "#2f7d4f",
  },
  {
    id: "harbor",
    name: "Harbor Havoc",
    description: "Salt air. Hot steel.",
    icon: "harbor",
    tint: "#2c6fa8",
  },
  {
    id: "quarry",
    name: "Dusty Dig",
    description: "Open pits. Dig a shortcut.",
    icon: "quarry",
    tint: "#a8733a",
  },
  // Extra levels are offered only with `?debug`. Each brings its own arena, bot
  // roster and rules from the engine (`crates/core/src/sim/extra_levels.rs`); the ids,
  // names and extra flags mirror `crates/core/src/sim/map_options.rs`.
  {
    id: "stress-test",
    name: "Stress Grid",
    description: "30 tanks · 75 destructibles · permanent buildings and barriers",
    icon: "stress",
    tint: "#6a5ea8",
    extra: true,
    teamTanks: 15,
  },
  {
    id: "superstress",
    name: "Scrap Yard",
    description: "Compact yard · 30 tanks · cover rebuilds and debris lingers",
    icon: "yard",
    tint: "#8a5a44",
    extra: true,
    teamTanks: 15,
  },
] as const;

export type MapOption = (typeof MAP_OPTIONS)[number];
export type MapId = MapOption["id"];
export type ExtraLevelId = Extract<MapOption, { extra: true }>["id"];

export const MAP_IDS: readonly MapId[] = MAP_OPTIONS.map((map) => map.id);

export function mapOption(id: string | null | undefined): MapOption | undefined {
  return MAP_OPTIONS.find((map) => map.id === id);
}

export function isExtraLevel(id: MapId): id is ExtraLevelId {
  return "extra" in mapOption(id)!;
}

/** Debug tools: Stats for nerds, extra levels in the map lists, and the console aids
 * `printDebugHelp` lists. A page opened with `?debug` has them, and so does every page
 * once Settings turns them on. */
export function debugPage(search: string): boolean {
  return new URLSearchParams(search).has("debug") || savedDebug();
}

/** The Settings dialog's Debug tools, remembered between visits (the `debug` player
 * preference, read here because this module imports nothing). */
export function savedDebug(): boolean {
  try {
    return globalThis.localStorage?.getItem("sloppy-debug") === "on";
  } catch {
    return false;
  }
}
