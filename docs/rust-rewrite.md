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

- [ ] Wave 1: core simulation port (+ tests)
- [ ] Wave 1: geometry library + all models/scenery (CPU)
- [ ] Wave 1: renderer foundation (PBR, shadows, fog, tone mapping, textures, instancing, custom effects)
- [ ] Wave 1: native server infrastructure (HTTP, WS + deflate, limits, catalog, monitor, dashboard)
- [ ] Wave 2: presentation + runtime effects (tanks, cover, fragments, particles, tracks, projectiles, HUD bars, cameras)
- [ ] Wave 2: net: replication, scene codec, match host, client timeline/interpolation
- [ ] Wave 2: web crate + TS shell (menus, UI, audio, controls, touch, network UI)
- [ ] Wave 3: remove Three.js/TS engine and Node server; content-version hashing for Rust; deploy scripts
- [ ] Wave 3: final verification (cargo fmt/clippy/test, pnpm check, browser checks, multiplayer, measurements)
