/** Battle Setup suggests a random bot name before the engine loads; the same list is the
 * bots' roster in `crates/core/src/sim/bot_personalities.rs`. */
const DEFAULT_NAMES = [
  "IRON JACK",
  "SIDEWINDER",
  "NITRO",
  "TREADHEAD",
  "HOTSHOT",
  "RIVET",
  "DUST DEVIL",
  "BULLSEYE",
  "SCRAP KING",
  "VEX",
  "BLACKTOP",
  "WRECKER",
  "FLINT",
  "GRIT",
  "BOLT",
  "ROAD RAGE",
  "CRATER",
  "SMOKESCREEN",
  "LOCKJAW",
  "RUMBLE",
  "CANNONBALL",
  "COPPERHEAD",
  "RUSTY",
  "BADGER",
  "DEADBOLT",
  "HELLCAT",
  "RICOCHET",
  "ROADBLOCK",
  "BUZZSAW",
  "CROWBAR",
  "THUNDERCLAP",
  "FLATLINE",
  "SLEDGE",
  "IRONCLAD",
  "REDLINE",
  "DIESEL",
  "DREADNOUGHT",
  "JUNKYARD",
  "SCORCH",
  "BRASS KNUCKLE",
  "WILDCARD",
  "HARDCASE",
  "RATTLER",
  "GHOST",
  "TOMBSTONE",
  "STEELTOE",
  "DUSTUP",
  "BOOMBOX",
  "HAILSTORM",
  "RAMPAGE",
  "SMOKESTACK",
  "AFTERSHOCK",
  "BACKFIRE",
  "BONEHEAD",
  "JACKHAMMER",
  "TORQUE",
  "WARBIRD",
  "BULLDOZER",
  "DYNAMO",
  "OUTLAW",
  "ROCKET DOG",
  "SIDESWIPE",
  "SPARKPLUG",
  "BARRAGE",
  "TANKBUSTER",
  "METALHEAD",
  "TRIGGER",
  "BLACKOUT",
  "WARPATH",
  "IRON WOLF",
  "SCATTERSHOT",
  "BOILER",
  "FUSE",
  "CRUNCH",
  "NIGHTSHIFT",
  "RUBBLE",
  "HEATWAVE",
  "HATCHET",
  "SHRAPNEL",
  "OVERDRIVE",
  "BULLWHIP",
  "SANDSTORM",
  "GUNSLINGER",
  "RIPSAW",
];

export function preferredPlayerName(): string {
  try {
    const saved = localStorage.getItem("sloppy-player-name")?.trim().slice(0, 24);
    if (saved) {
      return saved;
    }
  } catch {
    /* Optional preference. */
  }
  return DEFAULT_NAMES[crypto.getRandomValues(new Uint32Array(1))[0] % DEFAULT_NAMES.length];
}
export function rememberPlayerName(name: string): void {
  try {
    localStorage.setItem("sloppy-player-name", name);
  } catch {
    /* Optional preference. */
  }
}
