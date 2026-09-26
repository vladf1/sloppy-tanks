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
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ node scripts/hud-feedback-check.mjs
```

`npm run check:browser` runs every check below except `touch-loading-check`, one
after another, in about two minutes. Run it for startup, menu, input or rendering
changes. Browser checks keep only what needs a browser (real pointer, wheel,
keyboard and touch input, DOM and CSS, sounds, GPU rendering); rules that plain
simulation or Three.js objects can show belong in `tests/*.test.ts`.

Shared setup lives in `browser-helpers.mjs`: `launchGame()` opens Chrome with the
shared headless flag, `startRound()` starts rounds through the real Battle Setup
menu (the startup overlay otherwise swallows pointer and wheel input), and
`freezeLoop()` holds only the game's own `loop` callback so a check advances exact
frames while Three.js's animation callbacks keep running.

| Script                             | Verifies                                                                                                   |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `browser-check.mjs`                | Keyboard driving, mouse fire, tank choice, pause and zoom; stable WebGPU buffers across destructive resets |
| `startup-check.mjs`                | Menu before physics/GPU load, early GO with late choices, arena reuse, one atlas download, retry, layout   |
| `map-start-check.mjs`              | First frames on every map, both teams: no stale time, tanks at their spawns, no arrival tracks             |
| `hud-feedback-check.mjs`           | Wheel/key ammo selection, reticle, hit, rank, laser and pickup feedback, stable HUD layout, all sounds     |
| `touch-controls-check.mjs`         | Thumb sticks and simultaneous fingers, zoom, pause, touch preference and portrait hit-testing              |
| `round-recap-check.mjs`            | Battle reports, records across reloads, report layout; Solo Assault scoreboard, time limit and death       |
| `render-bundles-check.mjs`         | Cached draws match ordinary draws on all maps with moving and switched cameras                             |
| `destruction-check.mjs`            | Timber stages and breach, tree stumps and falling crowns, tower rubble textures, debris sink and fade      |
| `fixtures-check.mjs`               | PASS from the reinforcements, maps (switches, water reflections) and suspension fixtures                   |
| `multiplayer-simulation-check.mjs` | Two independently controlled seats, viewer cameras/bars, isolated speed sliders and literal player names   |
| `touch-loading-check.mjs`          | Touch UI code and styles load only when touch controls are enabled                                         |

`touch-loading-check.mjs` builds and serves its own production copy, because only
the production build splits the touch UI into separate files; it needs no dev
server:

```sh
node scripts/touch-loading-check.mjs
```

The dev server also serves interactive fixtures, listed on the dev site's
`/test-pages.html` (allowlist in `dev-site.ts`): `tests/*.browser.html`,
`tools/tank-surface-check.html`, `stresstest.html` and `superstress.html`. Fixtures with a pass/fail
verdict show it in a `#result` element starting with `PASS` or `FAIL`, which
`fixtures-check.mjs` reads.

## Multiplayer experiments

These checks default to the Vite URL `http://127.0.0.1:5173/sloppy-tanks/`; set
`SLOPPY_URL` for another. Room codes, the Chrome launch, physical clicks and
WebSocket-frame recording are shared in `multiplayer-helpers.mjs`. Server rules
(seats, idle watchdog, humans-only, reconnect, host transfer, expiry) are covered by
`tests/match-host.test.ts`, `tests/room-session.test.ts` and
`tests/player-controls.test.ts`; these checks cover what only a browser or a real
socket shows.

`npm run check:multiplayer-loading` builds and plays a production copy. It rejects
multiplayer requests, sockets or UI in single-player, server or traffic-bot code in
any browser chunk, and client simulation or Rapier JS/WASM downloads when opening
multiplayer. `tests/multiplayer-client-imports.test.ts` guards the same import
boundary in `npm test` and prints the offending import chain.
It also checks that a room link opens Battle Setup rather than a room page, that
multiplayer's extracted stylesheet loads only once a room is entered, that inactive
menu actions stay hidden, and that the menu fits desktop viewports.

`npm run check:multiplayer` drives **two Chrome contexts through real WebSockets**,
plus a third for a late join. Run Vite and the local server first (`npm run
server:dev`), or set `SLOPPY_SERVER=wss://45-63-56-58.sslip.io`. The host creates a
humans-only room on Battle Setup and the others open its room link, which selects
the room there. It covers literal names, synced read-only guest settings, the host's
humans-only checkbox through real pointer input, and two-player rounds without fill
bots. From the sent frames it checks that unchanged idle input goes out about once a
second without losing the seat, and that movement, aim, held fire and a single mine
keep the active cadence. It also measures button-to-visible movement and shot
feedback, then covers the idle menu and hidden tab, a reload that rejoins the same
seat from Battle Setup, late join and leave, results, restored bots on another map,
automatic reconnect and physical multi-touch driving, fire and mines.
`SLOPPY_LATENCY=50` (also 100/150) adds round-trip delay, `SLOPPY_JITTER=30` adds up
to 30 ms of variable delay, and `SLOPPY_STALL=200` holds about one message in fifty
and queues later ones behind it, like TCP head-of-line blocking. They set the dev
client's `?latency`, `?jitter` and `?stall` parameters, which also work on their own;
the added delay preserves message order. Stats for nerds shows the adaptive playout
buffer and the share of frames that ran past the newest snapshot. Browser
diagnostics exist only in dev builds.

`multiplayer-simulation-check.mjs`, included in `check:browser`, verifies two local
seats and viewer isolation without a server.

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

`node scripts/multiplayer-restart-check.mjs` starts an isolated local server on
port 8790 that admits the `SLOPPY_URL` origin, kills and restarts it during a round
(a crash, not a graceful stop) and checks that the browser returns to a fresh lobby
and prepares another map. Run `npm run server:build` first and run Vite.
`node --import tsx scripts/multiplayer-public-check.mjs` verifies the published dev
client (`SLOPPY_PUBLIC_URL`) with two players plus its test directory, fixture and
build metadata.

`npm run server:check:players` runs 4 real player sockets for 15 seconds per map,
including combat, reconnect and results. The traffic bots' `BotPlayer`
(`bots/bot-player.ts`) drives each socket; the check mirrors every snapshot to prove
the stream is contiguous and enforces full-state, snapshot-batch and sustained byte
budgets. Set `SLOPPY_SERVER_URL` for a deployed server (with a listed
`SLOPPY_ORIGIN`), `SLOPPY_PLAYER_CLIENTS=8` for the capacity check, or
`SLOPPY_PLAYER_SECONDS=900 SLOPPY_PLAYER_MAPS=village` for the long run.
Set `SLOPPY_PLAYER_RECOVER=1` to exercise recovery after transport loss: each
closure is retained in the report and room/seat continuity is mandatory. The
default run fails on any unexpected closure. This host test is manual, requires a
running server, and is outside CI.
Do not deploy or restart a watched server during a run: that resets rooms.

The client's projectile and lifecycle display timing is covered by
`tests/network-timeline.test.ts`; use human playtests to judge control feel.
Raw results, failures and screenshots belong under
`artifacts/performance/multiplayer/`. Automated timings do not establish player
comfort or account billing capacity; impact-to-feedback excludes projectile flight.

## Measurements

These are manual evidence, not regression gates. Keep them out of `npm run
check` and deployment workflows, run baseline and candidate workloads one at a
time on an otherwise idle machine, and report sample counts with outliers.

| Command                                               | Measures                                                                          |
| ----------------------------------------------------- | --------------------------------------------------------------------------------- |
| `node scripts/profile.mjs before` / `after`           | Matched runtime comparison with CPU profiles                                      |
| `node scripts/frame-pacing-check.mjs`                 | First gameplay frame and seeded combat on every map, without discarding a warm-up |
| `npm run benchmark:loading -- <label>`                | Cold-cache loading of a saved production build at 10 Mbps / 50 ms                 |
| `node --import tsx scripts/destruction-benchmark.ts`  | Headless destruction physics cost for a fixed wreck and blast scenario            |
| `npm run validate`                                    | Ten seeded headless matches and reset checks                                      |
| `node scripts/benchmarks/host-download-benchmark.mjs` | HTTP delivery from the live hosts only ([details](benchmarks/README.md))          |

- `profile.mjs` needs Vite running and `SLOPPY_URL`. Detailed results and CPU
  profiles go to `artifacts/performance/<label>/`.
- `frame-pacing-check.mjs` reads `SLOPPY_PACING_SECONDS`, `SLOPPY_MAX_FRAME_MS`
  and `SLOPPY_ARTIFACT_DIR`.
- For a loading comparison, save each production `dist` under
  `artifacts/performance/loading/<label>/` and keep each snapshot unchanged while
  measuring. Compare first content, menu appearance, final download and
  main-thread blocking separately.
- `destruction-benchmark.ts` takes an output path (default
  `artifacts/performance/destruction-benchmark.json`); compare a baseline and a
  candidate run.
- `npm run validate` writes `artifacts/performance/simulation-results.json`.
  Those are accelerated simulation results, not browser frame rates.

CPU submission times are not GPU timings, and local frame rates are not
guarantees for other devices.

## Assets, build and deployment

- Asset generators (`generate-*`, `optimize-textures.mjs`, `generate-previews.mjs`,
  `render-tank-previews.ts`) run through the npm
  commands in the root README's Assets section. `encode-webp.ts` is their shared
  lossless encoder.
- `startup-html.ts` inlines the Battle Setup menu into `index.html`, and
  `dev-site.ts` plus `deploy-dev.mjs` build and publish the dev site.
