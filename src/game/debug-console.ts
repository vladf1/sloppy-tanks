const PAGE_PARAMETERS = [
  "Page parameters (add to the URL):",
  "  ?debug               this help, nerd stats (N), extra levels in the map lists",
  "  ?webgl               the WebGL2 engine instead of WebGPU",
  "  ?map=<id>            preselect a map in Battle Setup (village, harbor, quarry, ...)",
  "  ?autoplay            skip Battle Setup; single player drives itself",
  "  ?latency=<ms>&jitter=<ms>&stall=<ms>   simulated network delay in a room",
];
const DEV_PARAMETERS = [
  "  ?server=ws://host:port   the game server a room connects to",
  "  ?tweak               single player's camera workshop",
];

/** Prints the page's debugging aids to the console once, on a `?debug` page. */
export function printDebugHelp(page: "single player" | "room"): void {
  const lines = [
    "Press N (or click “nerd stats”) for Stats for Nerds:",
    page === "room"
      ? "  the network (round trip, playout buffer, late batches, input acks), rendering, the battle."
      : "  physics, rendering, the battle and the configuration.",
    "",
  ];
  if (page === "room") {
    lines.push(
      "Room traffic, decoded to JSON (state arrives as binary frames of differences):",
      "  sloppy.wire.last(5)       the last 5 messages, received and sent",
      "  sloppy.wire.follow()      log each message as it comes; follow(false) stops",
      "  sloppy.wire.all()         every kept message (the last 300)",
      "",
    );
  } else if (import.meta.env.DEV) {
    lines.push(
      "Development build: window.sloppy controls the engine, for example",
      "  sloppy.debug()   sloppy.hud()   sloppy.stats()   sloppy.error()",
      "  sloppy.record() … sloppy.stop()   sloppy.autoplay()   sloppy.overview()",
      "  sloppy.giveAmmo()   sloppy.killHuman()   sloppy.soak(seconds)",
      "",
    );
  }
  lines.push(...PAGE_PARAMETERS, ...(import.meta.env.DEV ? DEV_PARAMETERS : []));
  console.info(`Sloppy Tanks debugging\n\n${lines.join("\n")}`);
}
