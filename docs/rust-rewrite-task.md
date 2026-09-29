# Task: complete Sloppy Tanks Rust/Wasm + WebGPU rewrite

> Historical task statement for the rewrite, kept for its contracts and completion
> criteria. The paths it names under `experiments/rust-webgpu/`, `server/` and the
> TypeScript engine were removed after the rewrite (they remain in version
> history); [rust-rewrite.md](rust-rewrite.md) describes the final state.

Implement the complete rewrite described below. This is an implementation task,
not a request for another proposal or another small demo. Make routine decisions
and keep working autonomously. The user is ambitious but wants efficient use of
model tokens and compute. Preserve the actual game, not merely its general idea.

The user explicitly authorizes subagents: use them whenever you judge they will
make the work faster. Delegate bounded, separable work (for example renderer
subsystems, simulation modules, server integration, or a late parity review),
agree on shared interfaces and file ownership, and integrate their results.
Avoid conflicting edits, duplicated exploration and multiple agents running the
same expensive suites. Apply the economical test cadence below to every agent.
You remain responsible for a coherent, complete implementation.

## Objective and scope

Replace the TypeScript game engine and Three.js renderer with a shared Rust game
core, Rapier physics integrated directly in Rust, and a custom Rust `wgpu` renderer
using handwritten WGSL. The browser runs the engine as WebAssembly. Complete the
multiplayer side using the same game core in a native Rust server on the existing
VPS architecture. Keep networking and authoritative simulation in the same server
process. The SERVER RUNTIME MUST BE FULLY RUST, including HTTP/WebSocket handling,
room hosting, authoritative simulation, validation, discovery and operator APIs.
Do not leave the Node/TypeScript server wrapping a Rust simulation as the finished
result. Browser and server must depend on ONE shared Rust implementation of game
rules and simulation; do not maintain duplicate JS/Rust implementations of the same
behavior. Temporary migration bridges must be removed before declaring completion.
Non-runtime build/deploy scripts and the browser DOM shell may remain JS where
appropriate. A native desktop client is a possible future benefit, NOT required here.

Preserve the game's current appearance, gameplay, controls, maps, progression,
menus, audio, single-player and multiplayer behavior. Retain the existing HTML/CSS
menus and page layout, with a thin JS/TS browser integration layer where useful.
There is no requirement to render DOM menus in WebGPU or rewrite every build script
in Rust. However, gameplay, AI, navigation, combat, authoritative simulation,
rendering, animation/effects and replication state must not remain implemented in
TypeScript behind a Rust wrapper. Browser API adapters for DOM, input, audio,
sockets and Wasm loading are fine; keep hot loops and game state in Rust.

End with ONE production engine and renderer. Temporary bridges during development
are fine. Do not leave permanent legacy/new-engine switches or two maintained
implementations. Keep the old implementation available as a Git/worktree baseline,
not as a second production runtime. Do not redesign the art, simplify maps, remove
features or weaken stress workloads to make the rewrite easier.

## Repository and isolation

- Repository: https://github.com/vladf1/sloppy-tanks.
- Reference implementation: [Rust/WebGPU experiment PR #25](https://github.com/vladf1/sloppy-tanks/pull/25). Its source is in `experiments/rust-webgpu/`, with setup and
  launch commands in that directory's `README.md`. All paths below are relative
  to the repository root unless stated otherwise.
- Inspect the current branch, HEAD and `git status --short`; main may have advanced
  since the experiment. Read current `AGENTS.md`, `README.md`, `scripts/README.md`
  and `server/README.md`.
- This task explicitly supersedes the Three.js/TSL requirement for the rewrite
  and changes TEST TIMING as described below. Other gameplay, resource,
  deployment and validation invariants still apply.
- Work in a separate worktree on a `codex/` branch. Check for existing rewrite
  work before starting duplicate efforts; preserve other ongoing work and
  unrelated changes. Bring in the reference PR's committed experiment as needed.
  Do not assume any particular local directory, running server or installed tools.
- Leave a runnable local result and a reviewable branch. Do not merge, publish,
  deploy, restart production rooms or create a public tunnel as part of this task.
  Prepare deployment support without executing production changes. Report clearly
  what is local, committed and unverified.

## Test cadence: keep it economical

The user explicitly says: **cool it with tests until you are confident the rewrite
is close to completion; do not burn tokens on repeated long-running tests.**

Apply that instruction concretely:

1. Read existing tests and capture a small useful baseline early. Reuse existing
   screenshots/artifacts when valid. Do not run a massive benchmark matrix before
   writing the implementation.
2. During implementation, use compiler/type errors, `cargo check`, and tiny targeted
   tests or browser smoke checks to resolve a SPECIFIC uncertainty. These are fine.
   Do not rerun the full JS suite, full browser catalog, every-map soak tests or
   broad performance benchmarks after every edit/module. Do not build a huge new
   testing framework as a prerequisite to progress.
3. Port/add focused deterministic tests alongside the relevant code when useful;
   defer comprehensive execution until the real implementation is integrated.
   Keep logs in files, inspect failures, and summarize counts instead of repeatedly
   dumping thousands of successful test lines into the conversation.
4. Once the game is substantially complete and the targeted smoke checks work,
   run the final quality gate and meaningful behavior/visual verification. Fix
   failures and rerun the affected checks. Broaden/repeat only for a concrete reason.
   This is a change to test cadence, NOT permission to ship an unverified rewrite.
5. Do not delete meaningful tests or loosen assertions merely to make the rewrite
   pass. Adapt tests to the new implementation; replace obsolete source-structure
   tests with relevant behavior coverage.

Keep a short durable progress/checklist file for continuation across context limits.
Avoid repeated repo scans, speculative framework design and parallel full builds.
When stuck, inspect the actual error/library source and fix the cause rather than
trying many random dependency/API combinations. Give concise progress updates.

## Working demo: reuse what is already proven

Start by reading the demo README, Cargo files, build script and source. It already
runs Rust physics AND Rust rendering in Chrome. No Three.js is involved.

- Rust 1.98.1; `wgpu = 30.0.0`; `rapier3d = 0.36.0`; `glam = 0.33`;
  `wasm-bindgen = 0.2.129`; CLI MUST match `wasm-bindgen = 0.2.129`.
  `Cargo.lock` pins the resolved versions. Reuse this known-working combination
  unless there is a concrete reason to change it.
- Browser-only dependencies are under `cfg(target_arch = "wasm32")`, so native
  physics tests do not need a graphics/window backend.
- wgpu features: `default-features = false`, `features = ["webgpu", "wgsl"]`.
  Browser uses `Backends::BROWSER_WEBGPU`. Preserve WebGPU-only behavior.
- Release profile: `opt-level = "s"`, `lto = true`, `codegen-units = 1`,
  `panic = "abort"`, `strip = true`. No extra `wasm-opt` pass was applied.
- `.cargo/config.toml` enables `-C target-feature=+simd128` for Wasm. The initial
  demo lacked this, but the current game uses SIMD Rapier. Keep SIMD explicit.
  Cargo config discovery depends on the invocation directory: the demo build
  script runs Cargo with the experiment directory as cwd.
- `build.mjs` runs `cargo build --locked --release --target wasm32-unknown-unknown`
  then `wasm-bindgen --target web`. Vite imports the generated glue and the Wasm
  through `?url`; initialize using `init({ module_or_path: wasmUrl })`.
  Vite's dev server still loads RELEASE Wasm. Rust/WGSL edits need a Wasm rebuild;
  Vite itself does not compile Rust on save.
- All generated output is under the repository's ignored
  `artifacts/performance/rust-webgpu/build/{target,pkg,dist}/`. The standalone Vite
  configuration keeps it separate from normal game builds/lint/deploy inputs.
- `src/physics.rs`: 60 Hz accumulator, previous/current pose interpolation,
  bounded projectiles and complete world reset. Its 100 ms frame cap is for a demo;
  choose catch-up/background behavior deliberately for the real game/server.
- `src/renderer.rs`: instanced cube draws, camera uniforms, shadow and main passes,
  4x MSAA, explicit sRGB output, persistent instance buffer, resize/disposal.
  `src/scene.wgsl` demonstrates working WGSL. This is a tiny renderer foundation,
  not existing support for all the game's materials/models/water/effects.
- `main.js`: thin browser controls, pause/reset, orbit, resize, visibility handling,
  error display and cleanup. `window.rustLab` exposes diagnostic results.
- Native physics tests passed; actual Chrome checks passed for release Wasm,
  pointer clicks, keyboard, collision response, pause/reset, orbit/zoom, resize,
  bounded launches, repeated resets and unsupported-WebGPU messaging.

### Portable toolchain setup

Check the available tools first and reuse compatible installations. The reference
experiment was verified with Rust 1.98.1. Install the `wasm32-unknown-unknown`
target and wasm-bindgen CLI 0.2.129 if missing; the CLI and crate versions must
match. Use rustfmt and clippy for final Rust checks. Follow the experiment README
for exact commands. No machine-specific environment variables are required.

Repo JS tooling is Node 24+ and pnpm 11+; the repository pins its exact pnpm through
`devEngines`. Do not use npm/npx. Run `pnpm install` in the new worktree.

### Specific traps already encountered

- An older wasm-bindgen/web-sys pin failed to compile wgpu 30 because `VideoFrame`
  was missing. The current 0.2.129 combination builds successfully.
- wgpu 30 differs from older tutorials: use
  `InstanceDescriptor::new_without_display_handle()`, optional vertex-buffer
  layouts/bind-group-layout entries, optional depth-write/compare fields,
  `multiview_mask`, `CurrentSurfaceTexture` and `queue.present(output)`.
  Copy the working renderer API usage or inspect the installed source.
- In this environment, popping a successful wgpu 30 error scope returned JS `null`
  and the library panicked in `Error::from_js` with `Unexpected error`. The working
  demo uses `on_uncaptured_error` and a device-loss callback, records the failure
  and stops/displays it. Preserve meaningful GPU error handling; do not silence it.
  Treat this as an observed version-specific problem, not a universal API claim.
- WGSL `fwidth` inside a non-uniform branch failed shader validation. Compute
  derivatives before the branch. A successful Rust compile does not validate all
  browser GPU shader/pipeline behavior.
- Match linear/sRGB formats and MSAA counts between pipelines and attachments.
  Destroy superseded size-dependent textures on resize. Reuse GPU buffers on reset.
- The demo's browser checker imports the existing headless Chrome helper; tests
  should not open desktop windows unless requested. Shader errors may appear only
  in console output. Physical pointer tests use `page.mouse`, not just DOM clicks.

## What the size experiment actually established

The demo includes a separate startup world (`src/feature_probe.rs`) exercising
11 observable checks for game-used Rapier API families. The outputs are consumed
in browser JS, so this is reachable release code, not dead test-only code:

- Convex hull construction (drums/barriers), triangle-mesh collision (rocks).
- Multiple colliders per body, collision/query group masks.
- Rotation locks, hard/soft CCD, mass, impulses including impulses at points.
- World raycasts with normals, ball/cuboid sweeps, overlap queries, local hull rays.
- Runtime shape replacement with cylinders, collider offsets/groups, body poses,
  sleep/wake and removals; contact-force event callbacks.

Measured bytes using the SAME Python gzip level 9 settings:

| Binary                              | Raw bytes | Gzip bytes |
| ----------------------------------- | --------: | ---------: |
| Original demo                       | 1,706,995 |    645,892 |
| Demo plus added Rapier APIs, scalar | 1,764,272 |    666,265 |
| Added APIs plus SIMD                | 1,861,913 |    646,346 |
| Existing game Rapier Wasm alone     | 2,021,200 |    761,373 |

The combined demo includes physics, renderer and shaders, but is not the whole
game. Most physics machinery was apparently already linked. Different Rapier
versions/build options mean these figures are not proof of equivalent behavior or
future full-game size/performance. The generic JS Rapier binding exports a broad
API; a Rust application can link only what it reaches. Do not claim guaranteed
large savings. These are historical measurements; generate fresh artifacts in your checkout
when measuring the rewrite. Do not assume the original local binaries are present.

Measured earlier: cached no-change build plus bindings ~0.26 s; recompiling just
the original demo plus bindings ~1.59 s. Rebuilding Wasm dependencies after enabling
SIMD later took 27.77 s (already-downloaded toolchain/crates). These are local
samples, not promises for full-game compilation.

## Existing game contracts that must survive

Inspect CURRENT source; do not rely on an old directory listing as the spec.
Useful entry points include `src/game/simulation.ts`, `render-state.ts`,
`presentation.ts`, `renderer.ts`, `part-batches.ts`, `batching.ts`, `water-surface.ts`,
`src/net/match-host.ts`, `server/room-session.ts`, and their existing tests.

- Fixed 1/60 simulation step and intentional update/contact/pickup/cleanup order.
  Rendering interpolation must never modify authoritative physics.
- Preserve seeded RNG algorithm AND draw order, bot-name stream separation,
  projectile earliest-contact resolution, team blocking/damage rules, old-life
  ordnance/XP attribution, navigation occupancy and destruction-chain semantics.
- JS numbers are doubles; do not indiscriminately change all game calculations
  to f32. Inspect where the physics boundary already rounds to f32. RNG integer
  overflow, rounding and transcendental differences can alter results.
- The demo uses a newer Rapier than the JS package; choose the full port's Rapier
  version deliberately. Do not infer Rust crate versions from the JS version
  number. Native/Wasm determinism is not automatic, especially with SIMD and
  changed numeric behavior. Preserve the authoritative-server model rather than
  introducing lockstep as an unrelated redesign. Distinguish genuine regressions
  from deliberate, documented physics-version differences.
- Human/bot command parity, one-shot input consumption, respawns, score/progression,
  reset/disposal, bounded enemies/fragments/events/effect pools.
- All maps and extra levels, including query-gated discovery and multiplayer
  behavior. Preserve Stress Grid and Scrap Yard's actual workloads, custom rules,
  map scaling, cover rebuilding and debris behavior.
- Match textured material response, lighting/shadows, fog, tone mapping, water
  reflections, foliage, terrain, particles, tracks, decals, tank suspension/recoil,
  damage/wreck stages, HUD and supported camera modes. Reuse existing art/audio.
- Current Three.js batching and render bundles already provide optimizations.
  A custom renderer must preserve batching, culling and bounded resource ownership.
  Avoid turning every model part into a separate draw or every entity into a
  JS/Wasm crossing. Prefer compact buffers and coarse browser calls.
- Preserve desktop and existing tablet/iPad controls. Phone support is out of scope.
- Keep menus responsive during async loading, handle early GO/retries/unavailable
  WebGPU, and compile/warm actual shadow/reflection/effect variants before they
  can cause first-use gameplay stalls.

## Multiplayer and deployment compatibility

The current dedicated Node server uses `MatchHost` and in-memory rooms behind
Caddy on the existing Vultr VPS. Read current server docs for exact routes,
settings and limits; they have changed since older notes. Preserve room discovery,
join/rejoin, host transfer, room lifecycle, privacy, validation/rate limits,
snapshot baselines/deltas/resync, input ordering and client interpolation.
Keep WebSockets; a new transport is not part of this rewrite.

The production and dev sites share the live server. Existing compatibility checks
hash shared source plus engine versions, and `serverBuild` separately fingerprints
server implementation. The current hash script uses the JS bundle import graph,
so it MUST be adapted for Rust shared sources, Cargo.lock/features and relevant
build inputs. Do not simply keep hashing obsolete TS files or bypass mismatch
rejection. Preserve traffic-bot compatibility or update its integration deliberately.

Implement native-server build/package/local-run and deployment-script support as
needed; keep loopback binding, Caddy/TLS and private operator endpoint protections.
No metered SaaS, additional remote simulation service, or automatic live rollout.
After future approval, browser/server deployment must be coordinated. Do not run
`deploy:dev` casually: it also deploys the shared VPS and resets live rooms.

## Suggested implementation sequence

1. Inventory current game features/seams and establish a minimal reference. Set up
   the isolated Rust workspace, builds and a concise completion checklist.
2. Use the proven demo to bring up the real Rust renderer. A temporary bridge from
   existing render state is fine; port representative tanks/scenery/materials,
   then every map/effect/camera/lifecycle path. WGSL and raw asset buffers carry over.
3. Port gameplay/simulation into shared Rust and integrate it with Rapier directly.
   Preserve behavior and remove JS physics access from hot loops. This can progress
   alongside rendering when the state contract is clear.
4. Integrate browser controls/UI/audio/network adapters and complete local play.
5. Port authoritative hosting to native Rust using the SAME core. Verify local
   multiplayer against the rewritten browser. Update builds/compatibility tooling.
6. Remove superseded production Three.js/TS engine paths/dependencies once all
   consumers are migrated. Retain useful external browser tooling and asset sources.
7. Run final comprehensive verification and fix regressions; report remaining
   limitations honestly. Do not declare completion after only the first map works.

Change the sequence if source evidence suggests a better route; don't spend the
whole task debating architecture. Keep dependencies lean and use ordinary Rust
structs/functions. Avoid introducing a full game framework/ECS unless clearly
needed by the existing workload.

## Completion criteria

- Production browser target runs the actual game with Rust/Wasm gameplay and
  custom Rust WebGPU rendering, without Three.js or the old TS simulation runtime.
  Remove `three`/`@types/three`, TSL and Three.js imports from the final project;
  migrate any remaining model/asset utilities that depend on them.
- All existing modes/maps/extra levels, controls, presentation and lifecycle paths
  are accounted for; no silent feature omissions.
- The server runtime is fully Rust, not a Node wrapper around Wasm/Rust. The old
  Node/TS server runtime and duplicate TS gameplay/replication implementations are
  removed from the production path and dependency graph after migration. Retain
  only genuine adapters/tooling, not two implementations hidden behind switches.
- Native Rust multiplayer server shares that core and works with real local clients,
  including reconnect/resync/room lifecycle. Build and compatibility checks work.
- Near completion: Rust formatting/lint/build and retained JS tooling checks pass;
  ported deterministic behavior checks and relevant existing tests pass; actual
  browser visual/input/reset/multiplayer checks pass. Use physical pointer inputs
  where relevant and inspect shader/console errors, not only successful builds.
- Run bounded final stress/reset checks and matched startup/runtime comparisons.
  Report raw/compressed total reachable bytes, readiness, frame-time tails and
  memory/resource behavior with sample counts. Separate local browser, network and
  server results; don't claim production performance from a local demo.
- Leave clear local launch/build/test instructions, a runnable browser URL, a concise
  change summary, validation evidence, risks and deployment steps. Keep logs and
  one-off screenshots/measurements under ignored artifacts. No live deployment.
