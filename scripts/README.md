# Scripts

Browser checks, measurements and asset tools. None of these run in CI: `npm run
check` covers lint, formatting, types, the build and `tests/*.test.ts`. A passing
gate does not verify controls, menu transitions, rendering or cleanup, so run the
matching browser check when changing those paths.

## Browser checks

Start `npm run dev` and pass the URL it prints. Every check drives installed
Google Chrome headless with an isolated profile, so no window takes focus; set
`SLOPPY_HEADED=1` to watch a check in a visible window. Each check exits non-zero
on failure and writes screenshots and results to the ignored `artifacts/performance/`.

```sh
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ npm run check:browser
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ node scripts/driving-check.mjs
```

`npm run check:browser` runs every check below except `touch-loading-check`, one
after another, in about six minutes. Run it for startup, menu, input or rendering
changes. Checks that open a round go through the real Battle Setup menu with
`startRound()` from `browser-helpers.mjs`; the startup overlay otherwise swallows
pointer and wheel input. Checks that control the game clock freeze only the
game's own `loop` callback and must leave Three.js's animation callbacks running.

| Script                             | Verifies                                                                                                 |
| ---------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `browser-check.mjs`                | Keyboard/mouse play, tank choice, pause and zoom; stable WebGPU buffers across ten destructive resets    |
| `startup-check.mjs`                | Menu before physics/GPU load, early GO with late choices, arena reuse, retry, previews, menu layout      |
| `map-start-check.mjs`              | First frames on every map and team: no stale time, tanks at their spawns, no arrival tracks              |
| `driving-check.mjs`                | Driving controls with real keyboard events and deterministic frames                                      |
| `ammunition-check.mjs`             | Wheel/number selection, zoom limits, held fire, mines, pause; crate and HUD visuals                      |
| `combat-feedback-check.mjs`        | Reload, hit and repair feedback, including sounds                                                        |
| `veterancy-check.mjs`              | Promotion after a real hit: rank label, toast, chevron and chime; rank reset on respawn                  |
| `laser-defense-check.mjs`          | Laser pickup timings and collection, HUD, rocket interception, pause and expiry                          |
| `projectile-visual-check.mjs`      | Every player munition model for both teams, size limits and capacity cap                                 |
| `touch-controls-check.mjs`         | Thumb sticks and simultaneous fingers; desktop keeps keyboard and mouse                                  |
| `round-recap-check.mjs`            | Battle report after END BATTLE, records across reloads, fits without scrolling, BATTLE SETUP             |
| `solo-survival-check.mjs`          | Reinforcements fixture, then solo scoreboard, pause, time limit and death                                |
| `bot-movement-browser.mjs`         | Bot retreat and head-on movement                                                                         |
| `render-bundles-check.mjs`         | Cached draws match ordinary draws on all maps with moving and switched cameras                           |
| `pickup-atlas-check.mjs`           | One pickup texture request, a shared face material, renders matching the source icons                    |
| `cover-hit-check.mjs`              | Shell hits and destruction for trees, timber and cargo: damage, events and particles                     |
| `tree-check.mjs`                   | Tree families, rooted stumps, falling crowns and bounded debris                                          |
| `tower-check.mjs`                  | Tower destruction: open navigation, split deck and posts, two distinct textured rubble piles             |
| `timber-walls-check.mjs`           | Every timber damage stage, the breach, and the intact neighbouring wall                                  |
| `debris-cleanup-check.mjs`         | Debris and wrecks sink and fade without shrinking                                                        |
| `multiplayer-simulation-check.mjs` | Two independently controlled seats, viewer cameras/bars, isolated speed sliders and literal player names |
| `touch-loading-check.mjs`          | Touch UI code and styles load only when touch controls are enabled                                       |

`touch-loading-check.mjs` needs a locally served production build, because only
the production build splits the touch UI into separate files:

```sh
npm run build
root=$(mktemp -d) && ln -s "$PWD/dist" "$root/sloppy-tanks"
python3 -m http.server 4179 --bind 127.0.0.1 --directory "$root"
SLOPPY_URL=http://127.0.0.1:4179/sloppy-tanks/ node scripts/touch-loading-check.mjs
```

The dev server also serves interactive fixtures, listed on the dev site's
`/test-pages.html` (allowlist in `dev-site.ts`): `tests/*.browser.html`,
`tools/scenery-check.html`, `tools/tank-surface-check.html` and `stresstest.html`.

## Multiplayer experiments

`npm run check:multiplayer-loading` builds and plays a production copy. It rejects
multiplayer requests, sockets or UI in single-player, server dependencies in any
browser chunk, and client simulation/WASM downloads when opening multiplayer.
It also checks that multiplayer's extracted stylesheet loads only in multiplayer,
inactive menu actions stay hidden, and the menu fits desktop viewports.

`node scripts/multiplayer-humans-only-check.mjs` checks the host's checkbox through
real pointer input, synced guest settings, two-player rounds without fill bots,
idle menu behavior, late joins, departure cleanup and restoring bots next round.
Use `SLOPPY_URL` and `SLOPPY_SERVER` as for `check:multiplayer`.
Set `SLOPPY_BASELINE_BUILD` to a saved pre-change build for emitted and actually
requested raw/gzip asset comparisons. It serves its own build; no dev server needed.

`node --import tsx scripts/multiplayer-room-browser-check.mjs` checks random/saved
names, responsive room listings, immediate creation on the selected map, Auto
teams, live player kills, join notifications, configurable match duration, late join,
received-update stats, polling cleanup and empty-room removal.
Use `SLOPPY_URL` for either the local Vite URL or the public dev site, and
`SLOPPY_CHECK_LABEL=public` to keep separate evidence. Buttons and room selection
use physical coordinate clicks.

`node --import tsx scripts/multiplayer-idle-input-check.mjs` measures unchanged
idle input over 6.5 seconds, verifies the seat stays human, and checks movement,
aim, fire, mine and resume delivery. It uses desktop Chrome and real
WebSocket frames. Set `SLOPPY_URL` and optionally `SLOPPY_SERVER`; use
`SLOPPY_CHECK_LABEL` to keep local/public evidence separate.

`multiplayer-simulation-check.mjs`, included in `check:browser`, verifies two local
seats and viewer isolation. `npm run check:multiplayer` instead drives **two Chrome
contexts through real WebSockets**: lobby, independent movement, fire, menus,
hidden-tab takeover, reconnect, results and another map. Run Vite and the local
server first (`npm run server:dev`), or set `SLOPPY_SERVER=wss://45-63-56-58.sslip.io`.
Use `SLOPPY_URL` for the Vite URL and `SLOPPY_LATENCY=50` (also 100/150) for added
round-trip delay. It also measures button-to-visible movement/shot feedback and
checks automatic reconnect and physical multi-touch driving/fire/mines. The client additionally accepts `?jitter=30` and
`?stall=200`, which holds about one message in fifty and queues later ones behind it,
like TCP head-of-line blocking; delayed channels preserve message order. Stats for nerds
shows the adaptive playout buffer and the share of frames that ran past the newest snapshot. Browser diagnostics exist only in dev builds.

`npm run server:check:players` runs 4 real player sockets for 15 seconds per map,
including combat, reconnect and results. Set `SLOPPY_SERVER_URL` for a deployed
server (with a listed `SLOPPY_ORIGIN`), `SLOPPY_PLAYER_CLIENTS=8` for the capacity check, or
`SLOPPY_PLAYER_SECONDS=900 SLOPPY_PLAYER_MAPS=village` for the long run.
`npm run server:check:lifecycle` verifies quiet rooms, simultaneous reconnect,
host transfer, socket revocation, slow readers, suspension and empty-room expiry.
Set `SLOPPY_PLAYER_RECOVER=1` to exercise recovery after transport loss: each
closure is retained in the report and room/seat continuity is mandatory. The
default run fails on any unexpected closure. These host tests are manual,
require a running server, and are outside CI.
Do not deploy or restart a watched server during a run: that resets rooms.

`node scripts/multiplayer-restart-check.mjs` starts an isolated local server on
port 8790, kills and restarts it during a round (a crash, not a graceful stop) and
checks that the browser returns to a fresh lobby and prepares another map. Run
`npm run server:build` first and run Vite.
`node --import tsx scripts/multiplayer-public-check.mjs` verifies the published dev
client with two players plus its test directory, fixture and build metadata.

`npm run check:latency` retains the original local 0/50/100/150 ms experiment,
three hull policies, jitter, 20/30 Hz input and overload checks. Its full-snapshot
combat sampling cannot represent very short projectile lifetimes. The real client
uses tick-stamped swept segments and intermediate lifecycle frames; its timing
regressions are in `tests/network-timeline.test.ts`. Use the real two-browser
check for current combat timing, and human playtests to judge control feel.

Raw results, failures and screenshots belong under
`artifacts/performance/multiplayer/`. Automated timings do not establish player
comfort or account billing capacity; impact-to-feedback excludes projectile flight.

## Measurements

These are manual evidence, not regression gates. Keep them out of `npm run
check` and deployment workflows, run baseline and candidate workloads one at a
time on an otherwise idle machine, and report sample counts with outliers.

| Command                                               | Measures                                                                                             |
| ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `node scripts/profile.mjs before` / `after`           | Matched runtime comparison with CPU profiles; writes `artifacts/performance-results.json` after both |
| `node scripts/frame-pacing-check.mjs`                 | First gameplay frame and seeded combat on every map, without discarding a warm-up                    |
| `npm run benchmark:loading -- <label>`                | Cold-cache loading of a saved production build at 10 Mbps / 50 ms                                    |
| `node --import tsx scripts/destruction-benchmark.ts`  | Headless destruction physics cost for a fixed wreck and blast scenario                               |
| `npm run validate`                                    | Ten seeded headless matches and reset checks                                                         |
| `node scripts/benchmarks/host-download-benchmark.mjs` | HTTP delivery from the live hosts only ([details](benchmarks/README.md))                             |

- `profile.mjs` needs Vite running and `SLOPPY_URL`. Detailed results and CPU
  profiles go to `artifacts/performance/<label>/`.
- `frame-pacing-check.mjs` reads `SLOPPY_PACING_SECONDS`, `SLOPPY_MAX_FRAME_MS`
  and `SLOPPY_ARTIFACT_DIR`.
- For a loading comparison, save each production `dist` under
  `artifacts/performance/loading/<label>/` and keep each snapshot unchanged while
  measuring. Compare first content, menu appearance, final download and
  main-thread blocking separately.
- `destruction-benchmark.ts` takes an output path; compare a baseline and a
  candidate run rather than the checked-in result.
- `npm run validate` rewrites the tracked `artifacts/simulation-results.json`.
  Those are accelerated simulation results, not browser frame rates.

CPU submission times are not GPU timings, and local frame rates are not
guarantees for other devices.

## Assets, build and deployment

- Asset generators (`generate-*`, `optimize-textures.mjs`, `generate-previews.mjs`,
  `render-tank-previews.ts`, `pack-tank-previews.mjs`) run through the npm
  commands in the root README's Assets section. `encode-webp.ts` is their shared
  lossless encoder.
- `startup-html.ts` inlines the Battle Setup menu into `index.html`,
  `copy-artifacts.mjs` copies top-level `artifacts/*.json` into builds, and
  `dev-site.ts` plus `deploy-dev.mjs` build and publish the dev site.
