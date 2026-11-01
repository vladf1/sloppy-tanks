# Scripts

Browser checks, measurements and asset tools. None of these run in CI: `bun run
check` covers formatting, lint, types, the site and server builds, clippy and the Rust
and Bun tests. A passing gate does not verify controls, menu transitions, rendering or
cleanup, so play the changed path in the page or the desktop app.

## Desktop UI gauntlet

This is a manual builder/critic loop against the web interface the egui shell replaced:
the TypeScript/DOM page's production build of `90423be0` (the last main commit before
PR 125, which production served until then), built locally and frozen, using native GPU
snapshots and that build's own JavaScript with identical fixture data. It covers
setup/loading, multiplayer setup, team/solo HUD, first person and damage, respawn,
pause/controls, Settings, team/solo results, room host/guest/rules/choices, room
pause/results/connection states, and both Stats for nerds panels.

Build the reference once per checkout (the capture checks the build's `/health`
commit):

```sh
git worktree add --detach artifacts/performance/web-shell/baseline-main 90423be0
(cd artifacts/performance/web-shell/baseline-main && pnpm install && pnpm run build)
```

`--reference-dist <dist>` names another copy of that build, and `--reference <url>` a
served page (the live site captured this way until production switched shells; it now
serves the egui page itself, which the capture cannot instrument).

Invoke the repository skill as `$ui-parity-gauntlet` in Codex or
`/ui-parity-gauntlet` in Claude Code. The shared instructions live in
[`.agents/skills/ui-parity-gauntlet`](../.agents/skills/ui-parity-gauntlet/SKILL.md);
`.claude/skills/ui-parity-gauntlet` is a relative symlink to that folder. Both
entry points stay in this repository, with no global skill installation.

```sh
bun run check:desktop-ui -- --round first
# After fixing the critic's findings, reuse the frozen reference:
bun run check:desktop-ui -- --round second
```

Run on a Mac with Metal and installed Chrome. The default sizes are 1280×800 and
1600×900 points at 2× resolution. `--sizes 1100x720` checks the native minimum too;
the web's stacked layout there is not a native acceptance requirement. `--out`
changes the evidence root. `--refresh-reference` deliberately captures the reference
again for changed fixtures (the same frozen build, so a fixture repair keeps the
reference's identity); do not use it just to clear a failed review. Build logs, fixture
JSON, PNGs, the reference's release/commit and asset hashes, randomized A/B contact
sheets and an HTML report stay under ignored `artifacts/performance/desktop-ui/`.

The capture serves the reference build on a local port, exposes its UI constructors in
intercepted response copies, holds gameplay after startup, and feeds those real
renderers the native snapshot fixtures. It changes no reference files and creates no
rooms.
Animation is held and the 3D arena is replaced by a flat backdrop in both shells.
This isolates shell fidelity; use a real app play-through for compositing, input,
animation and gameplay. The native Fullscreen/Quit controls and the platform's own
font rasterization are expected differences. A transient native joining card need
not reproduce the web's page-reload mechanics.

The lead agent acts as builder and integrator. After each capture, give a separate,
read-only critic the PNG contact sheets and the reference, without the builder's
self-assessment. The critic must inspect every scenario, record observable gaps
with priority and file/line evidence, and return **fix** or **accept within reason**.
Fix supported findings, run native tests/clippy, then capture a new round and judge
again. A completed capture records `needs-independent-review`, never an automatic
pass. Record the critic's verdict and any accepted platform differences beside
`round.json`; stop only when the evidence supports acceptance, the user interrupts,
or a real blocker is recorded. Screenshots are evidence, not a substitute for tests.

The stages also run separately:

```sh
SLOPPY_UI_SNAPSHOTS=artifacts/performance/desktop-ui/native cargo test --locked -p sloppy-desktop --profile desktop snapshots -- --ignored
bun scripts/desktop-ui-capture.mjs --native artifacts/performance/desktop-ui/native --out artifacts/performance/desktop-ui/reference
bun scripts/desktop-ui-report.mjs artifacts/performance/desktop-ui/native artifacts/performance/desktop-ui/reference artifacts/performance/desktop-ui/review
```

## Browser checks

The game page is the desktop crate's egui shell (`index.html`, `bun run wasm`), drawn on
one canvas, so no check drives it through the DOM: a check opens the page with
`?automation` and reads the shell's state and labelled widgets from `window.sloppyShell`
(`crates/desktop/src/automation.rs`), then plays it with real mouse, keyboard and CDP touch
input at those coordinates. Start `bun run dev` and pass the URL it prints. Checks drive
installed Google Chrome headless with an isolated profile, so no window takes focus; set
`SLOPPY_HEADED=1` to watch one in a visible window, and `SLOPPY_WEBGL=1` to play the
page's WebGL2 build. Each exits non-zero on failure and writes screenshots to the ignored
`artifacts/performance/<check>/`. Shared setup lives in `browser-helpers.mjs`
(`launchGame()`, `openShell()`, `untilState()`, `clickWidget()`, `fingers()`,
`tapWidget()`, `shellDebug()`); `bun run check:browser` runs them all
(`browser-checks.mjs`).

```sh
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ bun scripts/round-check.mjs
```

The room checks start their own game server on a free port from `SLOPPY_SERVER_BIN`
(default `target/server/sloppy-server`, which `bun scripts/build-server.mjs` builds with
the content version the page plays) and stage outages through `installProbes()` (dropped
sockets, a hidden page).

| Check                      | Plays                                                                                                                                                                                                                                                                          |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `shell-check.mjs`          | Battle Setup's choices, GO, drive, fire, zoom, V, pause, Settings, the report, a second round with the same bodies                                                                                                                                                             |
| `round-check.mjs`          | Every map's start at its spawns, Solo Assault, overview, a collapse, both reports, PLAY AGAIN; bodies and Wasm memory over resets                                                                                                                                              |
| `geometry-cache-check.mjs` | Each standard map in one profile: a first visit stores its vehicles and scenery, then warm visits alternately build them (`?geometry-cache=off`) and read them back; all draw the same frame with the same GPU resources, and reading back needs no larger Wasm heap (medians) |
| `first-person-check.mjs`   | V and the HUD's view button, pointer lock, mouse look per CSS pixel at 1x/2x (and as Safari), Esc twice, the retaking click, death and the respawn pick                                                                                                                        |
| `input-check.mjs`          | WASD/arrows, Space, ammo keys and wheel, Shift+wheel zoom, a right-click mine, blur, a hidden page, fullscreen, clicks at 2x and 1.5x                                                                                                                                          |
| `touch-check.mjs`          | A tablet's stick, arena fire, two fingers, mine, zoom keys, pause; Touch controls Auto/On/Off; a touch laptop                                                                                                                                                                  |
| `phone-check.mjs`          | The phone edition upright and sideways: Battle Setup's few choices, play by touch, pause, the report; `?phone`                                                                                                                                                                 |
| `webgl-check.mjs`          | The backend choice: WebGPU, `?webgl`, the fallbacks to WebGL2 and the failure view                                                                                                                                                                                             |
| `firefox-check.mjs`        | The installed Firefox, headless over WebDriver BiDi: startup, GO, driving, the pause (skipped without Firefox)                                                                                                                                                                 |
| `text-fields-check.mjs`    | Typing, clipboard shortcuts, Enter, Escape and IME text in the name and room code fields, and on a touch screen the focus a soft keyboard needs, in Chrome and WebKit (`webkit-tablet` runs one case)                                                                          |
| `multiplayer-check.mjs`    | Two players: create, invite link, join by link and by code, both drive, the dev delay, a socket drop, leaving, a reload's seat, a crash and restart, ROOM CLOSED, an outdated server's rooms                                                                                   |
| `lifecycle-check.mjs`      | Five single player → room → leave cycles on one page: sockets, listeners, GPU objects and Wasm memory stay bounded, no stuck key; a hidden page pauses, a hidden room keeps its seat                                                                                           |
| `webkit-check.mjs`         | Playwright WebKit at iPad and desktop sizes: startup, GO, W's frame rate, no late pipelines, frame gaps (printed; fails only on a stall over 250 ms)                                                                                                                           |
| `fixtures-check.mjs`       | The engine fixtures below                                                                                                                                                                                                                                                      |

The dev server serves interactive fixtures, listed on the dev site's
`/test-pages.html` (allowlist in `dev-site.ts`): `tests/*.browser.html` and
`tools/tank-surface-check.html`, plus a link to the game with `?debug`, whose
Battle Setup offers the Stress Grid and Scrap Yard. Each fixture runs the engine's
`Game` (`bun run wasm:engine`) on its own canvas (`tests/engine-fixture.ts`, through
`tools/game-engine.ts`) and arranges it through the `Game.debug_*` hooks
(`crates/web/src/game/debug.rs`): `reinforcements` (120 Solo Assault kills, bounded views,
reset and team mode), `maps` (inspection poses, crate stages, map switches, and both waters
reflecting a probe box in the framebuffer), `suspension` (acceleration, braking and
turning lean, a turret riding the hull's tilt), `humvee` (an orbit view of the TOW
humvee) and `destruction` (an interactive showcase of every destructible). Fixtures
with a pass/fail verdict show it in a `#result` element starting with `PASS` or
`FAIL`, which `fixtures-check.mjs` reads.

## Labs

The render lab, effects lab and tank previews use the labs build of the engine
(`RenderLab`, `EffectsLab`; the `labs` feature of `sloppy-web`), which the game never
loads: build it with `bun run wasm:labs` (into `src/generated/engine-labs/`),
then open the page on the dev server.

| Page or script                                          | Shows or checks                                                                                         |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `tools/render-lab.html`, `tools/render-lab-check.mjs`   | A calibration scene (PBR, fog, ACES, shadows, water, effects, joints, fades), reloads, culling, picking |
| `tools/effects-lab.html`, `tools/effects-lab-check.mjs` | A scripted scene with every effect and munition; bounded pools, no late pipelines, reset                |
| `tools/tank-surface-check.html`                         | The tank previews at card and gameplay scale                                                            |
| `tools/game-preview.html`                               | Single player on the game engine without menus (`bun run wasm:engine`)                                  |

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

Server rules (seats, idle watchdog, humans-only, reconnect, host transfer, expiry) are
covered by the engine's tests (`crates/core/tests/integration/net_match_host.rs`,
`net_player_controls.rs`, `crates/server/tests/`). Room state travels in binary WebSocket
frames, deltas against what the client already holds; `wire-view.mjs` decodes them
through the engine Wasm (one `WireView` per socket, so run `bun run wasm:engine` first)
into the JSON `full` and `snapshot` shapes, and `state-mirror.mjs` applies those and
asserts the snapshot stream stays contiguous.

`bun run server:check:players` runs 4 real player sockets for 15 seconds per map,
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

For real loss below TCP on a Mac, `sudo scripts/network/lossy-network.sh on` drops 2% of
the packets from the server ([details](network/README.md)).

The client's projectile and lifecycle display timing is covered by
`crates/core/tests/integration/net_timing.rs`; use human playtests to judge control feel.
Raw results, failures and screenshots belong under
`artifacts/performance/multiplayer/`. Automated timings do not establish player
comfort or account billing capacity; impact-to-feedback excludes projectile flight.

## Measurements

These are manual evidence, not regression gates. Keep them out of `bun run
check` and deployment workflows, run baseline and candidate workloads one at a
time on an otherwise idle machine, and report sample counts with outliers.

| Command                                                                                  | Measures                                                                               |
| ---------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `cargo run --release -p sloppy-core --example capture_benchmark`                         | Multiplayer host physics, scene capture, diff and encoding per room timer interval     |
| `cargo run --release -p sloppy-server --example snapshot_bandwidth`                      | Room bytes raw and on the wire, deflate, decode and projectile share; state parity     |
| `cargo run --release -p sloppy-core --example simulation_benchmark`                      | Headless seeded autoplay tick time on one map                                          |
| `cargo run --release -p sloppy-core --example bot_skill`                                 | Per-role bot accuracy, damage, kills, stalls and hit response in bots-only matches     |
| `cargo run --release -p sloppy-web --example allocation_benchmark -- output.json [seed]` | Native allocation requests/bytes and stage timings, plus wire/render/HUD parity hashes |
| `bun run validate`                                                                       | Ten seeded headless matches and reset checks                                           |
| `bun scripts/benchmarks/host-download-benchmark.mjs`                                     | HTTP delivery from the live hosts only ([details](benchmarks/README.md))               |

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
- `bun run validate` writes `artifacts/performance/simulation-results.json`.
  Those are accelerated simulation results, not browser frame rates.

CPU submission times are not GPU timings, and local frame rates are not
guarantees for other devices.

## Assets, build and deployment

- Asset generators (`generate-*`, `optimize-textures.mjs`, `generate-previews.mjs`)
  run through the Bun commands in the root README's Assets section.
  `encode-webp.ts` is their shared encoder (lossless for data tiles, lossy for noisy
  colour artwork such as the drum), `texture-noise.ts` their seeded
  value noise, and `asset-data.ts` holds the ammunition labels and colors, pickup colors
  and atlas layout they paint with (the engine's own copies are in `crates/core`; change
  both together).
- `generate-previews.mjs` renders the tank selection previews from the game's vehicle
  models with the labs engine (`tools/tank-surface-check.html`; `bun run generate:previews`
  builds that engine first) and packs `public/previews/tanks.webp`.
  `SLOPPY_PREVIEWS_OUT` writes a candidate elsewhere for comparison. The renderer has no orthographic camera or
  transparent canvas, so a narrow perspective camera far away frames the view and
  each tank is drawn over black and over white to recover its coverage; a soft
  contact shadow, the track footprint projected through the same view, is painted
  under each tank.
- `generate-desktop-icon.mjs` (`bun run generate:desktop-icon`, on any OS) derives
  `assets/desktop/mac-app.icns` from the web app's tank icon; `generate:app-icons`
  regenerates it after the web icons. The checked-in icon and its
  source hashes let desktop packaging copy it without invoking Cargo or installing
  the image library in CI; see `assets/texture-sources/README.md`.
- `dev-site.ts` plus `deploy-dev.mjs` build and publish the dev site.
