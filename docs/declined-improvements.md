# Declined improvements

These ideas were measured or prototyped while the egui shell became the web page
(PR 125 and PR 127, October 2026), and then set aside. Each one still looks worth
doing; it is listed with what it would gain, why it waited, and what would make it
worth picking up again. The numbers were measured then and on one machine (Chrome
on a Mac unless noted), so re-measure before relying on them.

## Load time and download size

### Split the page's Wasm into a menu half and an engine half

- **Gain:** on a first visit at 10 Mbit/s, Battle Setup appeared at about 1.2 s
  instead of 2.2 s; on fast links 40–75 ms sooner. The arena was ready no sooner.
- **How:** binaryen's `wasm-split` cuts the one shell module in two that share one
  memory. The menu half (about 884 KB) draws Battle Setup while the engine half
  downloads, then the engine adopts the menu's GPU device and canvas.
- **Why it waited:** it doubles every shell build, adds a pinned binaryen download
  to the build, and the function partition and GPU/UI handover were the most
  fragile code of the load-time work (a review found a WebGL fallback race that
  only exists with the split). About 50–65 KB of engine data still shipped in the
  menu half, because `wasm-split` moves only functions.
- **Revisit when:** first visits on slow mobile links matter. Split only release
  builds and keep an unsplit fast build for development.
- **Where:** local branch `claude/load-trio-split`, including a `split-check` browser
  check.

### Subset the interface fonts

- **Gain:** about 65–100 KB Brotli on a first visit, the largest lossless-in-practice
  cut left.
- **Why it waited:** on hold by choice. It needs a coverage test proving every
  character the game, player names and room codes can show is kept.

### Re-encode two textures lossily

- **Gain:** about 49 KB: `dry-grass.webp` from quality 75 to 65 saves 20 KB
  (−0.8 dB PSNR, no visible change at 2× zoom), and `village-ground.webp` near-lossless
  saves 29 KB (colours off by at most 2/255).
- **Why it waited:** it trades a small amount of quality, which was declined then.
  These two files are 37% of the texture download.

### Measure tanks before painting them

- **Gain:** Battle Setup's first frame 12–19 ms sooner (the tank dimensions it needs
  come from the unpainted geometry).
- **Why it waited:** a warm arena was ready 5–9 ms later, against the rule that
  nothing else gets slower.

### Start the arena's texture downloads beside the engine download

- **Gain:** production's former page did this. It should help slow mobile links,
  where the arena waits on textures.
- **Why it waited:** on cold loads at 10 and 50 Mbit/s it did not pay off against
  fetching them when the arena build starts, which shipped instead.

### Shorten WGSL identifiers

- **Gain:** perhaps 15–20 KB Brotli of shader text.
- **Why it waited:** the renderer looks some names up at run time, so it needs a
  WGSL-aware minifier and care. Comments are already stripped; removing whitespace
  saves only about 4 KB.

## Interface

### Text drawn by the browser

- **Gain:** egui text rasterized through the canvas's font engine, closer to the
  former page's type.
- **Why it waited:** Anton Wide was good enough. The prototype still needs the tag
  fit fix and bold at weight 800.
- **Where:** local branch `browser-glyphs`; screenshots and timings in
  `artifacts/performance/parity-2026-10-30/fonts/browser-glyphs/`.

### Remaining differences from the former web page

- **Joining a room:** the former page stayed on Battle Setup with "JOINING…" on the
  button and the status under it. The egui page shows a "Joining room" card over
  Battle Setup's background.
- **Crosshair cursor** over the arena, as the former page had.
- **Stats for nerds** in a monospace font. It needs another embedded font for a
  debug-only panel.

## Measured and not worth it

Recorded so they are not retried without a new reason:

- **Splitting each map into its own module:** about 125 KB Brotli (3%), but the
  physics, the server and seeded matches share the map code.
- **`wasm-opt`, and stripping the Wasm's `name` and `producers` sections:** the
  Brotli download got bigger.
- **Remapping source paths in panic messages:** 1.2 KB.
- **Re-encoding the lossless textures and the preview's transparency:** already at
  maximum compression.
- **Combining the textures into one file:** under 1%, since WebP is already
  compressed.
- **Delaying the sound downloads:** a round's first sound could come out silent.
- **Shortening the menu's 250 ms fade:** the menu is already drawn and clickable
  under it.
