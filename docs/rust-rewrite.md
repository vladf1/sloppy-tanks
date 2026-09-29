# Rust/Wasm + WebGPU rewrite: progress

Branch `codex/rust-rewrite`. Task statement: `experiments/rust-webgpu/REWRITE-TASK.md`.
Baseline: `main` at 35afd91 (TypeScript + Three.js + Rapier JS 0.20, Node server).

## Layout

| Crate                  | Target         | Owns                                                                  |
| ---------------------- | -------------- | --------------------------------------------------------------------- |
| `crates/core`          | native + wasm  | `sim/` gameplay, `geometry/` meshes, `scene.rs` model contract, `models/`, `net/` protocol/replication/match host |
| `crates/render`        | wasm (wgpu)    | WebGPU renderer, WGSL, presentation and effects                       |
| `crates/web`           | wasm cdylib    | wasm-bindgen API for the page: game loop, commands, HUD state, network client state |
| `crates/server`        | native binary  | HTTP/WebSocket server, rooms, limits, monitor, dashboard              |

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
