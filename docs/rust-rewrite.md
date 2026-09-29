# Rust/Wasm + WebGPU rewrite: progress

Branch `codex/rust-rewrite`. Task statement: `experiments/rust-webgpu/REWRITE-TASK.md`.
Baseline: `main` at 35afd91 (TypeScript + Three.js + Rapier JS 0.20, Node server).

## Layout

| Crate           | Target        | Owns                                                                                                              |
| --------------- | ------------- | ----------------------------------------------------------------------------------------------------------------- |
| `crates/core`   | native + wasm | `sim/` gameplay, `geometry/` meshes, `scene.rs` model contract, `models/`, `net/` protocol/replication/match host |
| `crates/render` | wasm (wgpu)   | WebGPU renderer, WGSL, presentation and effects                                                                   |
| `crates/web`    | wasm cdylib   | wasm-bindgen API for the page: game loop, commands, HUD state, network client state                               |
| `crates/server` | native binary | HTTP/WebSocket server, rooms, limits, monitor, dashboard                                                          |

Toolchain: Rust 1.98.1 (`rust-toolchain.toml`), `wasm32-unknown-unknown`, wasm-bindgen CLI 0.2.129.
Rapier 0.36 (Rust) replaces Rapier JS 0.20; physics differences are a deliberate version change.

## Decisions

- Game math stays f64 like the JS numbers; Rapier is f32 at the physics boundary.
- The seeded Mulberry32 stream keeps its algorithm, double-valued state and draw order.
- Wire protocol stays JSON with the same message shapes, so traffic bots keep working.
- Models are CPU node trees in core: the renderer draws them and the simulation measures them.

## Checklist

- [x] Wave 1: core simulation port (+ 232 tests; construction/RNG parity exact vs TS)
- [x] Wave 1: geometry library + vehicle models (bit-exact vs Three r185)
- [x] Wave 1b: cover/tree/prop models; map scenery; integration and de-duplication with sim
- [x] Wave 1: renderer foundation (PBR, shadows, fog, tone mapping, textures, instancing, custom effects)
- [x] Wave 1: native server infrastructure (HTTP, WS + deflate, limits, catalog, monitor, dashboard)
- [x] Wave 2: runtime effects (pools, lab vs Three.js)
- [ ] Wave 2: model effect WGSL (flag-cloth, wreck-aging, debris-fade, pickup-surface, meadow-sway, chimney-smoke, quarry-soil, sandstone, sand-drift)
- [ ] Wave 2: presentation (tanks, cover, fragments, particles, tracks, projectiles, HUD bars, cameras)
- [ ] Wave 2: net: replication, scene codec, match host, client timeline/interpolation
- [ ] Wave 2: web crate + TS shell (menus, UI, audio, controls, touch, network UI)
- [x] Content-version hashing for Rust; static musl server build; VPS deploy scripts
- [ ] Wave 3: remove Three.js/TS engine and Node server
- [ ] Wave 3: final verification (cargo fmt/clippy/test, pnpm check, browser checks, multiplayer, measurements)

## Status notes (2026-09-29)

- Running when paused: presentation + single-player Game bindings (worktree agent),
  networking/match host port (worktree agent). Merge their branches next.
- Next: model effect shaders; wire `presentation/model_catalog.rs` to real models;
  TS shell on the Game API; multiplayer client bindings; remove Three.js/TS engine and
  Node server; final verification. Toolchain PATH must put rustup first:
  `export PATH=/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH`.
- Presentation + Game API finished on branch `worktree-agent-a75d18446fab95167`
  (commits fe54bd9..0aeefe1), not merged yet: its `effects/mod.rs` is a stub that the
  merged effects port replaces; `presentation/model_catalog.rs` still returns placeholder
  models and must be wired to `models::{build_scenery, cover_model, tree_model, ...}`.
  Release wasm 2.76 MB raw / 0.98 MB gzip; shadow pass ~150 draws (needs cross-model batching).
- Networking finished on branch `worktree-agent-a4f9dd187621c52fd` (2b9a8aa..51dcf68), not
  merged yet: core `net/` (protocol, codec, replication, MatchHost, NetworkClient state
  machine), server `MatchRoom` adapter; golden wire test vs the TS host is structurally
  identical; real-socket smoke with TS traffic bots passed. Merge, then bind NetworkClient
  in `crates/web` and replace the TS multiplayer client internals.

## Single-player shell

`src/game.ts` drives the wasm `Game` (`crates/web/src/game.rs`); `src/engine.ts` loads
the glue and takes over the binary download that production pages start in `<head>`
(the `engine-download` plugin in `vite.config.ts`). `src/game/engine-api.ts` holds the
packed input and frame-result slots and the JSON shapes of `hud_json`, `drain_events`
and `stats_json`. `Controls` and the touch sticks only gather raw input
(`Controls.takeInput`); the engine builds commands. The HUD, menus, battle report,
audio and Stats for nerds read engine JSON. `tests/single-player-imports.test.ts`
fails if single player reaches Three.js, Rapier JS or any TypeScript engine module.

Legacy bridges for the TypeScript multiplayer client, to delete with it:
`Controls.command()` and the four-argument `NerdStats` constructor.

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
