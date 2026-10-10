# Scripts

Browser checks, measurements and asset tools. None of these run in CI: `pnpm run
check` covers lint, formatting, types, the build and `tests/*.test.ts`. A passing
gate does not verify controls, menu transitions, rendering or cleanup, so run the
matching browser check when changing those paths.

## Browser checks

Start `pnpm run dev` and pass the URL it prints. Every check drives installed
Google Chrome headless with an isolated profile, so no window takes focus; set
`SLOPPY_HEADED=1` to watch a check in a visible window. `webkit-startup-check`
drives Playwright's own WebKit build (Safari's engine, with WebGPU) instead; install
it once with `pnpm exec playwright install webkit`. Each check exits non-zero
on failure and writes screenshots and results to the ignored `artifacts/performance/`.

```sh
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ pnpm run check:browser
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ node scripts/hud-feedback-check.mjs
```

Checks play on the WebGPU engine where the browser has it. `SLOPPY_WEBGL=1` hides
WebGPU from their pages, so the same checks run on the WebGL2 engine through the
page's own fallback:

```sh
SLOPPY_WEBGL=1 SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ pnpm run check:browser
```

`pnpm run check:browser` runs every check below except `touch-loading-check`, one
after another, in a few minutes. Run it for startup, menu, input or rendering
changes. Browser checks keep only what needs a browser (real pointer, wheel,
keyboard and touch input, DOM and CSS, sounds, GPU rendering); rules the simulation
or presentation can show natively belong in the Rust tests (`cargo test`) or
`tests/*.test.ts`.

Shared setup lives in `browser-helpers.mjs`: `launchGame()` opens Chrome with the
shared headless flag, `startRound()` starts rounds through the real Battle Setup
menu (the startup overlay otherwise swallows pointer and wheel input), and
`freezeLoop()` holds the game's one `loop` animation callback, which hands the packed
input to `Game.frame`, so a check advances exact engine frames.

Checks read and arrange the engine through the dev-only `window.sloppy` (see
[below](#windowsloppy-development-builds)) and the `Game.debug_*` fixture hooks
(`crates/web/src/game/debug.rs`): an emptied arena, placed and patched tanks, damage
through the shared damage paths, pickups, shells, mines, fixed simulation steps, still
frames from a fixed camera, a pixel probe and two room seats. `launchGame()` installs
`window.engine` in every page: `state()` (simulation and camera), `view()` (what every
entity's view showed in the last frame: reticle rings, health-bar chevrons, cover
stages and stumps, pickup podiums, debris opacity, laser beams, effect counts),
`covers()`, `stats()` (renderer counters: draws, pipelines, late pipelines,
allocations), `draw(camera)` and `setTank`/`setHuman`/`setSim` patches.

| Script                             | Verifies                                                                                                                                                                                                                                                            |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `browser-check.mjs`                | Keyboard driving, mouse fire, tank choice, pause and zoom; rubble without late pipelines; stable renderer allocations and live WebGPU buffers across destructive resets and mines                                                                                   |
| `first-person-check.mjs`           | V/◎ toggle, mouse turns the turret view, W follows the view, click fire, first Esc frees the cursor and a second pauses, death keeps the pointer, Esc then a physical respawn choice, overhead restore                                                              |
| `startup-check.mjs`                | Menu before physics/GPU load, early GO with late choices, arena reuse, one atlas download, retry, layout                                                                                                                                                            |
| `map-start-check.mjs`              | First frames on every map, both teams: no stale time, tanks at their spawns, no arrival tracks                                                                                                                                                                      |
| `hud-feedback-check.mjs`           | Wheel/key ammo selection, reticle, hit, rank, laser and pickup feedback, stable HUD layout, all sounds                                                                                                                                                              |
| `touch-controls-check.mjs`         | Tablet and car-screen touch: drive stick, arena aim and fire, mine and ammo taps while firing, first person, zoom, pause, preference and layout at four sizes                                                                                                       |
| `phone-check.mjs`                  | Emulated phone: tank, map and tabs only in Battle Setup, an Easy team battle, drive stick, first person, pause and zoom only, zoom taps, landscape and portrait hit-testing                                                                                         |
| `round-recap-check.mjs`            | Battle reports, records across reloads, report layout; Solo Assault scoreboard, time limit and death                                                                                                                                                                |
| `render-cameras-check.mjs`         | Every map through moving, overview, first-person, zoomed and fixed cameras: no late or new pipelines, and a still frame matches pixel for pixel after a detour through other views                                                                                  |
| `destruction-check.mjs`            | Timber stages and breach, scars on loose members, tree stumps and falling crowns, distinct tower rubble, debris sink and fade                                                                                                                                       |
| `fixtures-check.mjs`               | PASS from the reinforcements, maps (switches, mesh page slack, water reflections) and suspension fixtures                                                                                                                                                           |
| `multiplayer-simulation-check.mjs` | Two room seats driven through `PlayerControls`, each drawn from its own viewer (camera, models, bars); isolated speed sliders and literal player names                                                                                                              |
| `webgl-check.mjs`                  | The WebGL2 engine (`?webgl`) on every standard map: only its binary downloads, a round drives, fires and draws with shadows without errors; the page picks the build the browser supports by itself; a failing WebGPU device falls back to WebGL on the same canvas |
| `webkit-startup-check.mjs`         | WebKit: ready menu on the village and quarry with at most 150 distinct pipelines, a physical GO click, W driving at 45+ fps and a mouse shot, no page, console or GPU errors or late pipelines                                                                      |
| `touch-loading-check.mjs`          | Touch UI code and styles load only when touch controls are enabled                                                                                                                                                                                                  |

`touch-loading-check.mjs` builds and serves its own production copy, because only
the production build splits the touch UI into separate files; it needs no dev
server:

```sh
node scripts/touch-loading-check.mjs
```

The dev server also serves interactive fixtures, listed on the dev site's
`/test-pages.html` (allowlist in `dev-site.ts`): `tests/*.browser.html` and
`tools/tank-surface-check.html`, plus a link to the game with `?debug`, whose
Battle Setup offers the Stress Grid and Scrap Yard. Each fixture runs the engine's
`Game` on its own canvas (`tests/engine-fixture.ts`) and arranges it through the
debug hooks: `reinforcements` (120 Solo Assault kills, bounded views, reset and team
mode), `maps` (inspection poses, crate stages, map switches, and both waters
reflecting a probe box in the framebuffer), `suspension` (acceleration, braking and
turning lean, a turret riding the hull's tilt), `humvee` (an orbit view of the TOW
humvee) and `destruction` (an interactive showcase of every destructible). Fixtures
with a pass/fail verdict show it in a `#result` element starting with `PASS` or
`FAIL`, which `fixtures-check.mjs` reads.

### `window.sloppy` (development builds)

| Member                                        | Backed by / meaning                                                        |
| --------------------------------------------- | -------------------------------------------------------------------------- |
| `game`                                        | The wasm `Game` itself                                                     |
| `sim`, `view` (getters), `debug()`            | `debug_json()`: a fresh copy per read (match, human, tanks, camera, zoom…) |
| `hud()`, `stats()`, `snapshot()`              | `hud_json()`, `stats_json()`, `debug_snapshot()`                           |
| `error()`                                     | `error()`: the first GPU error or null                                     |
| `frames`, `events`                            | Engine frames run and events drained by the page                           |
| `audio`, `controls`                           | The page's `AudioSystem` and `Controls`                                    |
| `start()`, `restart()`                        | A fresh round now / a fresh world behind Battle Setup                      |
| `autoplay(v)`, `overview(v)`, `autoRounds(v)` | `debug_set_autoplay/overview/auto_rounds`                                  |
| `zoom(z)`, `reflections(v)`, `firstPerson()`  | `debug_set_zoom`, `debug_set_reflections`, `toggle_first_person`           |
| `giveAmmo(n)`, `killHuman()`                  | `debug_give_ammo`, `debug_kill_human`                                      |
| `stress()`, `collapse()`, `soak(s)`           | `debug_stress`, `debug_collapse`, `debug_soak` (synchronous)               |
| `record()`, `stop()`, `report()`, `samples`   | Frame recorder (per-frame `stats_json` while recording)                    |
| `exactResolution()`                           | `resize(2560, 1440, 1, true)` until reload                                 |

`sim` and `view` are read-only snapshots: checks that assigned simulation fields
(`sim.human.ammo.rocket = 10`) use the methods instead. Zoom from the wheel or touch
buttons reaches the engine with the next frame's input, so checks wait a frame.
`profile.mjs` uses `game.debug_configure(seed, tanks, team)` and
`game.debug_stress_burst()`.

## Labs

The render lab, effects lab and tank previews use the labs build of the engine
(`RenderLab`, `EffectsLab`; the `labs` feature of `sloppy-web`), which the game never
loads: build it with `pnpm run wasm:labs` (into `src/generated/engine-labs/`),
then open the page on the dev server.

| Page or script                                          | Shows or checks                                                                                         |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `tools/render-lab.html`, `tools/render-lab-check.mjs`   | A calibration scene (PBR, fog, ACES, shadows, water, effects, joints, fades), reloads, culling, picking |
| `tools/effects-lab.html`, `tools/effects-lab-check.mjs` | A scripted scene with every effect and munition; bounded pools, no late pipelines, reset                |
| `tools/tank-surface-check.html`                         | The tank previews at card and gameplay scale                                                            |
| `tools/game-preview.html`                               | Single player on the game engine without menus (the normal build)                                       |

The labs compare the engine with reference frames of the same scenes drawn by the
game's former Three.js r185 renderer, captured before Three.js left the project. They
are too large to commit (about 0.9 MB as lossless WebP), so they live in the ignored
`artifacts/references/labs/` (`render-{default,overhead,close,resized}.png`,
`effects-{village,quarry,quarry-close}.png`, with the capture's own numbers in
`reports.json`); keep a copy. Without them the labs draw and report the engine only
and the difference panel stays blank. With them `render-lab-check.mjs` requires each
pose's mean error to stay within `RENDER_LAB_TOLERANCE` (default 1.0 of 255;
calibrated at 0.16 default, 0.25 overhead, 0.15 close-up, 0.15 resized) and
`effects-lab-check.mjs` within `EFFECTS_LAB_TOLERANCE` (default 4; calibrated at 1.86
village, 2.08 quarry, 4.22 close-up, where the two sides' cosmetic randomness differs).

## Multiplayer experiments

These checks default to the Vite URL `http://127.0.0.1:5173/sloppy-tanks/`; set
`SLOPPY_URL` for another. Room codes, the Chrome launch, physical clicks and
WebSocket-frame recording are shared in `multiplayer-helpers.mjs`. Server rules
(seats, idle watchdog, humans-only, reconnect, host transfer, expiry) are covered by
the engine's tests (`crates/core/tests/net_match_host.rs`, `net_player_controls.rs`,
`crates/server/tests/`); these checks cover what only a browser or a real socket
shows. Room state travels in binary WebSocket frames, deltas against what the client
already holds; `wire-view.mjs` decodes them through the engine Wasm (one `WireView`
per socket, so run `pnpm run wasm` first) into the JSON `full` and `snapshot` shapes.
Checks that follow a room's state apply those with `state-mirror.mjs`, which also
asserts the snapshot stream stays contiguous.

`pnpm run check:multiplayer-loading` builds and plays a production copy. It rejects
multiplayer requests, sockets or UI in single-player, server or traffic-bot code in
any browser chunk, and any download of a TypeScript simulation or Rapier JS/WASM when
opening a room: rooms run on the engine Wasm. `tests/multiplayer-client-imports.test.ts`
guards the import boundary in `pnpm test` and prints the offending import chain.
It also checks that a room link opens Battle Setup rather than a room page, that
multiplayer's extracted stylesheet loads only once a room is entered, that inactive
menu actions stay hidden, and that the menu fits desktop viewports.

`pnpm run check:multiplayer` drives **two Chrome contexts through real WebSockets**,
plus a third for a late join. Run Vite and the local server first (`pnpm run
server:dev`), or set `SLOPPY_SERVER=wss://sloppy-tanks-server.fridman.me`. The host creates a
humans-only room on Battle Setup and the others open its room link, which selects
the room there. It covers literal names, room rules shown as text to guests and during
play, the host's bot setting, and two-player rounds without fill bots. From the sent frames it checks that unchanged idle input goes out about once a
second without losing the seat, and that movement, aim, held fire and a single mine
keep the active cadence. It also measures button-to-visible movement and shot
feedback, then covers the idle menu and hidden tab, a reload that rejoins the same
seat from Battle Setup, late join and leave, results, restored bots on another map,
automatic reconnect and physical multi-touch: driving, arena fire, mines and first-person turning.
`SLOPPY_LATENCY=50` (also 100/150) adds round-trip delay, `SLOPPY_JITTER=30` adds up
to 30 ms of variable delay, and `SLOPPY_STALL=200` holds about one message in fifty
and queues later ones behind it, like TCP head-of-line blocking. They set the dev
client's `?latency`, `?jitter` and `?stall` parameters, which also work on their own;
the added delay preserves message order. A dev page opened with `?latencySlider` shows
sliders for all three in a room, to feel the controls at a changing delay while driving. Stats for nerds (on a `?debug` page) shows the adaptive playout
buffer and the share of frames that ran past the newest snapshot. Browser
diagnostics exist only in dev builds.

Those parameters delay messages inside the client, so TCP never loses anything, and
Chrome DevTools' packet loss only affects WebRTC. For real loss below TCP on a Mac,
`sudo scripts/network/lossy-network.sh on` drops 2% of the packets from the server
([details](network/README.md)).

`multiplayer-simulation-check.mjs`, included in `check:browser`, verifies two local
seats and viewer isolation without a server.

With Vite and the local server running, `player-feedback-check.mjs` checks the
online HUD's power-up timers, critical hull and death explanation using controlled
HUD fixtures over a rendered room. `player-preferences-check.mjs` checks saved
tank, mode, camera and zoom through page reloads, another round, and single-player
to multiplayer transitions. Both write desktop screenshots and results under
`artifacts/performance/player-ux/`:

```sh
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ node scripts/player-feedback-check.mjs
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ node scripts/player-preferences-check.mjs
```

`node --import tsx scripts/multiplayer-room-browser-check.mjs` checks random/saved
names, responsive room listings, immediate creation on the selected map, Auto
teams, live player kills, join notifications, configurable match duration, late join,
received-update stats, polling cleanup and empty-room removal. For both the reload
and in-page joins it samples every frame: Battle Setup (restored after the reload)
stays until the arena replaces it, and the room menu never shows. A link to a room
that isn't open opens Battle Setup, says so and selects nothing.
Use `SLOPPY_URL` for either the local Vite URL or the public dev site,
`SLOPPY_SERVER` as for `check:multiplayer`, and `SLOPPY_CHECK_LABEL=public` to keep
separate evidence. Buttons and room selection
use physical coordinate clicks.

With Vite and the local server running, `node scripts/phone-multiplayer-check.mjs` plays
phones' one-action Multiplayer tab: a phone that sees no open room creates one on its map
with bots, and a second phone, starting on single player, has that room picked with its
map shown, joins through a reload, and gets the phone camera, controls and short room
menu. Each phone's room list keeps only the rooms the check made, so other rooms on the
server cannot change the pick.

`node scripts/multiplayer-restart-check.mjs` starts an isolated native server
(`target/server/sloppy-server`) on port 8790, or `SLOPPY_RESTART_PORT`, that admits the
`SLOPPY_URL` origin, kills and restarts it during a round
(a crash, not a graceful stop) and checks that the connection dialog shows, that the
browser returns to a fresh lobby and prepares another map, and that a graceful stop
then shows the room-closed dialog with its way back to Battle Setup. Run
`pnpm run server:build` first and run Vite.
`node --import tsx scripts/multiplayer-public-check.mjs` verifies the published dev
client (`SLOPPY_PUBLIC_URL`) with two players plus its test directory, fixture and
build metadata.

`pnpm run server:check:players` runs 4 real player sockets for 15 seconds per map,
including combat, reconnect and results. The traffic bots' `BotPlayer`
(`bots/bot-player.ts`) drives each socket; the check mirrors every snapshot to prove
the stream is contiguous and enforces full-state, snapshot-batch and sustained byte
budgets. Set `SLOPPY_SERVER_URL` for a deployed server,
`SLOPPY_PLAYER_CLIENTS=8` for the capacity check, or
`SLOPPY_PLAYER_SECONDS=900 SLOPPY_PLAYER_MAPS=village` for the long run.
Set `SLOPPY_PLAYER_RECOVER=1` to exercise recovery after transport loss: each
closure is retained in the report and room/seat continuity is mandatory. The
default run fails on any unexpected closure. This host test is manual, requires a
running server, and is outside CI.
Do not deploy or restart a watched server during a run: that resets rooms.

The client's projectile and lifecycle display timing is covered by
`crates/core/tests/net_timing.rs`; use human playtests to judge control feel.
Raw results, failures and screenshots belong under
`artifacts/performance/multiplayer/`. Automated timings do not establish player
comfort or account billing capacity; impact-to-feedback excludes projectile flight.

## Measurements

These are manual evidence, not regression gates. Keep them out of `pnpm run
check` and deployment workflows, run baseline and candidate workloads one at a
time on an otherwise idle machine, and report sample counts with outliers.

| Command                                                                                  | Measures                                                                               |
| ---------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `node scripts/profile.mjs before` / `after`                                              | Matched runtime comparison with CPU profiles                                           |
| `node scripts/frame-pacing-check.mjs`                                                    | First gameplay frame and seeded combat on every map, without discarding a warm-up      |
| `pnpm run benchmark:loading -- <label>`                                                  | Cold-cache loading of a saved production build at 10 Mbps / 50 ms                      |
| `cargo run --release -p sloppy-core --example capture_benchmark`                         | Multiplayer host physics, scene capture, diff and encoding per 50 ms room interval     |
| `cargo run --release -p sloppy-server --example snapshot_bandwidth`                      | Room bytes raw and on the wire, deflate, decode and projectile share; state parity     |
| `cargo run --release -p sloppy-core --example simulation_benchmark`                      | Headless seeded autoplay tick time on one map                                          |
| `cargo run --release -p sloppy-core --example bot_skill`                                 | Per-role bot accuracy, damage, kills, stalls and hit response in bots-only matches     |
| `cargo run --release -p sloppy-web --example allocation_benchmark -- output.json [seed]` | Native allocation requests/bytes and stage timings, plus wire/render/HUD parity hashes |
| `pnpm run validate`                                                                      | Ten seeded headless matches and reset checks                                           |
| `node scripts/benchmarks/host-download-benchmark.mjs`                                    | HTTP delivery from the live hosts only ([details](benchmarks/README.md))               |

- `profile.mjs` needs Vite running and `SLOPPY_URL`. Detailed results and CPU
  profiles go to `artifacts/performance/<label>/`.
- `frame-pacing-check.mjs` reads `SLOPPY_PACING_SECONDS`, `SLOPPY_MAX_FRAME_MS`
  and `SLOPPY_ARTIFACT_DIR`.
- For a loading comparison, save each production `dist` under
  `artifacts/performance/loading/<label>/` and keep each snapshot unchanged while
  measuring. Compare first content, menu appearance, final download and
  main-thread blocking separately.
- `capture_benchmark` seeds each standard map and both extra levels with one idle
  player, warms up 1200 ticks, then times 400 intervals of three steps (each
  followed by the host's projectile path recording) and a snapshot. It takes an
  output path after `--`.
- `snapshot_bandwidth` plays the Village (with bot fill), the Stress Grid and the
  Scrap Yard with four scripted players that drive and fire like the traffic bots,
  through `MatchHost` and a server-role codec per connection (permessage-deflate
  with context takeover). It reports raw and on-the-wire bytes per message type, the
  host's interval, deflate and native client decode times, projectile path entries
  and their raw share, and hashes every frame each client projects, so two builds
  can show they replicate the same state. It takes a label, the measured seconds
  (60) and comma-separated seeds (4242) after `--`; `SLOPPY_VERIFY=1` also compares
  every client's mirror with the host after every frame, and `SLOPPY_DUMP=1` writes
  every message sent. Bytes repeat exactly for a seed, so one run per build compares
  them; interval times drift between runs like every timing here.
- `simulation_benchmark` takes a map id and a seed after `--`; it warms up 600
  ticks, times 3600 and prints JSON. For an engine change, run a base-commit
  worktree and the candidate alternately over several maps and seeds.
- `bot_skill` plays seeded bots-only team matches on the standard maps (eight
  seeds each by default; `--maps`, `--seeds`) and writes
  `artifacts/performance/bot-skill.json`. Pass an earlier output as `--baseline`
  to print differences. An AI change alters every seeded match, so judge it by the
  means ± two standard errors over rounds, not by any one match.
- `allocation_benchmark` runs five multiplayer maps, with 1200 warm-up ticks and
  400 measured three-tick intervals per map. It retains every sample. Allocation
  counts include reallocations; bytes are requested sizes, not live or peak heap.
  `timelinePush` measures enqueueing an already constructed frame. `hud` measures
  the human and scoreboard blocks. Compare matching seeds and output hashes from
  saved baseline/candidate executables in alternating order. These instrumented
  native timings do not measure browser render CPU or FPS.
- `pnpm run validate` writes `artifacts/performance/simulation-results.json`.
  Those are accelerated simulation results, not browser frame rates.

CPU submission times are not GPU timings, and local frame rates are not
guarantees for other devices.

## Assets, build and deployment

- Asset generators (`generate-*`, `optimize-textures.mjs`, `generate-previews.mjs`)
  run through the pnpm commands in the root README's Assets section.
  `encode-webp.ts` is their shared lossless encoder, and `asset-data.ts` holds the
  colors, labels and pickup atlas layout they paint with (the engine's own copies
  are in `crates/core`; change both together).
- `generate-previews.mjs` renders the tank selection previews from the game's vehicle
  models with the labs engine (`tools/tank-surface-check.html`; run `pnpm run wasm --
--labs` first) and packs `public/previews/tanks.webp`. `SLOPPY_PREVIEWS_OUT` writes
  a candidate elsewhere for comparison. The renderer has no orthographic camera or
  transparent canvas, so a narrow perspective camera far away frames the view and
  each tank is drawn over black and over white to recover its coverage. The
  checked-in sheet is still the Three.js rendering: the engine's differs by about 13
  of 255 on tank pixels (brighter sides), so it has not been regenerated.
- `startup-html.ts` inlines the Battle Setup menu into `index.html`, and
  `dev-site.ts` plus `deploy-dev.mjs` build and publish the dev site.
