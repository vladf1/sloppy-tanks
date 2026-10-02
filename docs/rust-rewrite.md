# Rust/Wasm + WebGPU rewrite

The game engine moved from TypeScript, Three.js and Rapier JS to one Rust
implementation shared by the browser (Wasm + WebGPU) and the native multiplayer
server. Baseline: `main` at 35afd91 (TypeScript + Three.js r185 + Rapier JS 0.20,
Node server). Task statement: [rust-rewrite-task.md](rust-rewrite-task.md); its
reference experiment (`experiments/rust-webgpu/`, PR #25) was removed once
superseded and remains in Git history.

## Layout

| Crate           | Target        | Owns                                                                                                              |
| --------------- | ------------- | ----------------------------------------------------------------------------------------------------------------- |
| `crates/core`   | native + wasm | `sim/` gameplay, `geometry/` meshes, `scene.rs` model contract, `models/`, `net/` protocol/replication/match host |
| `crates/render` | wasm (wgpu)   | WebGPU renderer, WGSL, presentation and effects                                                                   |
| `crates/web`    | wasm cdylib   | wasm-bindgen API for the page: `Game`, `NetGame`; `RenderLab`/`EffectsLab` with the `labs` feature                |
| `crates/server` | native binary | HTTP/WebSocket server, rooms, limits, monitor, dashboard ([guide](../crates/server/README.md))                    |

The TypeScript left in `src/` is the page shell: menus, HUD, input gathering,
touch controls, audio and the room page's DOM. The Node server (`server/`), the
TypeScript engine and its Node tests, `three`, `@types/three`,
`@dimforge/rapier3d-simd` and `@dimforge/rapier3d-simd-compat` are gone.

Toolchain: Rust 1.99.0 (`rust-toolchain.toml`), `wasm32-unknown-unknown`,
`x86_64-unknown-linux-musl`, wasm-bindgen CLI 0.2.129. Rapier 0.36 (Rust)
replaces Rapier JS 0.20; physics differences are a deliberate version change.
`pnpm run wasm:labs` builds the development labs into
`src/generated/engine-labs/`; production builds never include them.

## Decisions

- Game math stays f64 like the JS numbers; Rapier is f32 at the physics boundary.
- The seeded Mulberry32 stream keeps its algorithm, double-valued state and draw order.
- Wire protocol stays JSON with the same message shapes, so traffic bots keep working.
- Models are CPU node trees in core: the renderer draws them and the simulation measures them.

## Test mapping

The TypeScript engine's Node tests left with their subjects. Their behavior is
covered in Rust (`cargo test --workspace --release`, 502 tests in 48 binaries on
2026-09-29); golden fixtures recorded from the TypeScript engine
(`crates/core/tests/fixtures/`) pin construction parity and the wire format.

| Removed TypeScript test                                                                                              | Rust coverage                                                                                              |
| -------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `ammunition`, `bot-movement`, `cover-destruction`, `cover-hit-effects`, `debris-cleanup`                             | `crates/core/tests/` files of the same names (`ammunition.rs`, `bot_movement.rs`, …)                       |
| `destruction-physics`, `difficulty`, `driving`, `game`, `hit-registration`, `humvee`, `laser-defense`                | `crates/core/tests/` files of the same names                                                               |
| `map-layout`, `modes`, `movable-barrels`, `navigation`, `personalities`, `quarry`, `shoot-mines`, `simulation-setup` | `crates/core/tests/` files of the same names                                                               |
| `stress-test-level`, `superstress-level`, `tank-contact`, `tank-destruction`, `timber-walls`, `veterancy`            | `crates/core/tests/` files of the same names                                                               |
| `render-state`                                                                                                       | `crates/core/tests/render_state.rs`                                                                        |
| `extra-levels-multiplayer`                                                                                           | `crates/core/tests/net_extra_levels.rs`                                                                    |
| `match-host`, `traffic-bots` (host half)                                                                             | `crates/core/tests/net_match_host.rs`, `net_golden.rs`; `tests/traffic-bots.test.ts` now uses the server   |
| `multiplayer-simulation`, `player-controls`                                                                          | `crates/core/tests/net_player_controls.rs`                                                                 |
| `replication`, `scene-capture`                                                                                       | `crates/core/tests/net_replication.rs`                                                                     |
| `network-clock`                                                                                                      | `crates/core/tests/net_timing.rs` (fixed-step clock, input cadence, playout and render timelines)          |
| `batching`, `part-batches`                                                                                           | `crates/render`: `draw_list`, `model`, `shadow_merge` tests; `crates/core/src/models/tests*.rs`            |
| `flags`, `tree-damage`, `village-scenery`, `pickup-atlas`                                                            | `crates/core/src/models/tests_props.rs`, `tests_scenery.rs` (`flags_match_typescript`, `tree_damage_…`)    |
| `projectile-visuals`, `explosion-effects`, `tracks`, `track-dust`                                                    | `crates/render/src/effects/` unit tests (`projectiles`, `explosions`, `particles`, `tracks`, `track_dust`) |
| `first-person`, `tank-suspension`                                                                                    | `crates/render/src/presentation/first_person.rs`, `suspension.rs`, `camera_rig.rs` tests                   |
| `renderer`, `renderer-resources`, `prepare-scene`, `loading-assets`, `bundle-stats`                                  | `crates/render/src/shader.rs` and `effects/registry.rs` (every WGSL variant validates); browser checks     |
| `rate-limit`, `room-catalog`, `room-session`, `server-monitor`                                                       | `crates/server/src/`: `rate_limit`, `room_catalog`, `session_tests`, `monitor` tests                       |
| `node-server`                                                                                                        | `crates/server/tests/server.rs`, `match_room.rs`                                                           |
| `lazy-rapier-wasm`                                                                                                   | None: the Vite plugin it tested is gone with Rapier JS                                                     |

Kept TypeScript tests (`pnpm test`): audio, button input, controls, game options,
Stats for nerds, round recap, startup errors, task yield, touch input, the two
import boundaries, and the traffic bots against the built Rust server.
`scripts/validate.ts` became `cargo run --release -p sloppy-core --example validate`
(`pnpm run validate`); the simulation and capture benchmarks became the
`simulation_benchmark` and `capture_benchmark` examples; the destruction benchmark
was not ported.

## Quality gate

`pnpm run check` runs Prettier and `cargo fmt` checks, the Wasm build, `tsc` for
the shell/tests/scripts/tools and bots, the Vite build, ESLint, clippy with
`-D warnings` (workspace natively with all targets, `sloppy-render` and `sloppy-web`
on wasm32, and `sloppy-web` with `labs`), the native server build, `pnpm test` and
`cargo test --workspace` (dev profile, dependencies optimized). CI
(`.github/workflows/check.yml`) runs the same scripts as four parallel jobs: site,
Rust format and clippy, Rust tests, and server with the page-shell tests. It
installs the pinned toolchain through rustup and a cached wasm-bindgen-cli 0.2.129
(`.github/actions/setup`), with one Cargo cache per job that main's deploy run keeps
warm for pull requests.

## Download size

Production `dist/index.html` plus `dist/assets/*` (all JavaScript, CSS and Wasm the
game and room pages can load; textures, audio and previews are unchanged), gzip -9 and
Brotli quality 11, measured locally on 2026-09-29 against the baseline build:

| Build                             | Files | Raw bytes | Gzip bytes | Brotli bytes |
| --------------------------------- | ----: | --------: | ---------: | -----------: |
| Baseline (TS + Three + Rapier JS) |    19 | 3,636,073 |  1,144,674 |      882,570 |
| Rust engine                       |    12 | 3,578,926 |  1,255,594 |      944,799 |

The final build is one engine binary (`engine_bg-*.wasm`: 3,329,339 raw, 1,177,893 gzip,
876,849 Brotli) and about 250 KB raw of page shell. The baseline split its engine
into `graphics` (Three.js, 813,764 raw), `physics` (157,655) plus the Rapier Wasm
(2,196,730) and the TypeScript engine chunks. Compressed, the Rust build is 7–10%
larger; the labs feature keeps about 170 KB raw of render/effects lab code out of it.

## Single-player shell

`src/game.ts` drives the wasm `Game` (`crates/web/src/game.rs`); `src/engine.ts` loads
the glue and takes over the binary download that production pages start in `<head>`
(the `engine-download` plugin in `vite.config.ts`). `src/game/engine-api.ts` holds the
packed input and frame-result slots and the JSON shapes of `hud_json`, `drain_events`
and `stats_json`. `Controls` and the touch sticks only gather raw input
(`Controls.takeInput`); the engine builds commands. The HUD, menus, battle report,
audio and Stats for nerds read engine JSON. `tests/single-player-imports.test.ts`
and `tests/multiplayer-client-imports.test.ts` allowlist the shell modules each
page may load. The room page (`src/net/client.ts`) drives `NetGame` the same way,
sharing `Controls.takeInput` and `NerdStats` (with `network-stats.ts` rows).

`pnpm run wasm` must run before `tsc`, `vite build` or the dev server: `src/engine.ts`
imports the generated `src/generated/engine/`.

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
| `zoom(z)`, `firstPerson()`                    | `debug_set_zoom`, `toggle_first_person`                                    |
| `giveAmmo(n)`, `killHuman()`                  | `debug_give_ammo`, `debug_kill_human`                                      |
| `stress()`, `collapse()`, `soak(s)`           | `debug_stress`, `debug_collapse`, `debug_soak` (synchronous)               |
| `record()`, `stop()`, `report()`, `samples`   | Frame recorder (per-frame `stats_json` while recording)                    |
| `exactResolution()`                           | `resize(2560, 1440, 1, true)` until reload                                 |

`sim` and `view` are read-only snapshots: checks that assigned simulation fields
(`sim.human.ammo.rocket = 10`) use the methods instead. Zoom from the wheel or touch
buttons reaches the engine with the next frame's input, so checks wait a frame.
`scripts/profile.mjs` uses `game.debug_configure(seed, tanks, team)` and
`game.debug_stress_burst()`.

## Status (2026-09-29)

Feature-complete and verified locally (PR #26, draft until a coordinated browser+server
release). `pnpm run check` passes (516 Rust, 36 TS tests); `check:browser` passes 12 checks
including headless WebKit; multiplayer checks pass against a local Rust server.

Deliberate differences from the TypeScript baseline: Rapier 0.36 contact response; zlib-rs
level 2 WebSocket compression (fewer bytes than the old level 1); square cargo fittings on
the three moored harbor ships (GPU cost); ownerless damage sent as `owner: 0`.

Known trade-offs: the page downloads one engine Wasm (≈0.89 MB Brotli), so single player is
~10% larger compressed and a room entry fetches the whole engine (prefetched from Battle
Setup). Not done: regenerating `public/previews/tanks.webp` with the Rust renderer (its
candidate is ~13/255 brighter), and a first CI run with Rust happened on the PR only.
Matched measurements are in the ignored `artifacts/performance/rust-rewrite/final/`.
