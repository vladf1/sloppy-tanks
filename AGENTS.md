# Sloppy Tanks development guide

This file records the project rules that are easy to violate and expensive to
rediscover. Use `README.md` for player-facing behavior, setup and deployment,
and `scripts/README.md` for the browser-check and measurement catalog; do not
turn this file into a second directory listing.

## Before changing code

- Check `git status --short` first. Preserve existing user changes and do not
  rewrite unrelated work.
- Trace the behavior from input or simulation state to presentation before
  editing. Make the smallest change that fixes the observed problem, and add
  a focused regression test when the behavior is testable without a browser:
  a Rust unit test beside the code or a `crates/*/tests/` file for engine,
  net and server behavior, `tests/*.test.ts` for the page shell.
- Establish a reproducible failure or a matched before/after measurement
  before doing a broad refactor or performance change.
- Keep tests fast and deterministic. Prefer fixed-step loops, scoped random
  mocks, and an in-memory clock over real sleeps or deleting meaningful
  coverage because a visual effect is flaky.

## Layout

One implementation of the game, in Rust, shared by the browser and the server:

- `crates/core` (native and Wasm): `sim/` simulation, rules, bots, navigation,
  maps and levels on Rapier; `geometry/` and `models/` meshes and model trees
  that the simulation measures and the renderer draws; `net/` protocol,
  replication, match host and client connection state.
- `crates/render` (Wasm): the shared WebGPU (`wgpu`) and direct WebGL2 (`glow`) renderer, WGSL shaders
  (`src/shaders/`, `src/presentation/shaders/`), presentation, effects, cameras
  and input commands. Pure CPU parts compile natively and carry its tests.
- `crates/web` (Wasm cdylib): the wasm-bindgen API. `Game` runs single player
  and `NetGame` a room page, one coarse call per frame; the `labs` feature adds
  the development `RenderLab` and `EffectsLab`, and `webgl` (without the default
  `webgpu`) makes the WebGL2 fallback engine.
- `crates/server` (native): the multiplayer server ([guide](crates/server/README.md)).
- `src/` is the TypeScript page shell only: menus, HUD, input gathering, touch
  controls, audio and the room page's DOM. Game rules, simulation, rendering,
  effects and replication must not come back into TypeScript; the import
  boundary tests (`tests/*-imports.test.ts`) allowlist the shell modules.

## Code style

- Code should be easy to trace from a player action to its simulation result
  and visible feedback. Prefer descriptive domain names (`tank`, `simulation`,
  `command`, `brain`), small single-purpose functions, and plain structs,
  enums and functions over traits or abstractions that merely forward calls.
  Short coordinates, loop indices and conventional math names are fine inside
  small calculations.
- Name balance values, timeouts, capacities and tolerances. Shared combat rules
  live in `crates/core/src/sim/combat_rules.rs`, physics/lifecycle settings in
  `sim/simulation_rules.rs`, camera and feedback timing in
  `crates/render/src/presentation/view_settings.rs`; settings used by one
  algorithm stay beside it. Geometry, palettes, authored map placements and test
  expectations are data: keep them in their model, layout or fixture.
- Units are metres, seconds and radians unless a name says otherwise; DOM and
  performance timers use milliseconds. X/Z is the playable plane and Y is up.
  `alpha` is the interpolation fraction between previous and current poses.
  Game math is `f64` like the former JavaScript numbers; Rapier and GPU buffers
  are `f32` at their boundaries. Do not narrow gameplay math to `f32` wholesale.
- Comments explain intent and invariants (why a query is ordered, why a
  resource is shared), not what an assignment does. Rust comments that name a
  `.ts` file refer to the TypeScript baseline the code was ported from
  (`35afd91`, in Git history).
- Keep the Wasm boundary coarse: packed `Float32Array` input and frame results,
  JSON for HUD, events and stats, never per-entity calls from JavaScript.
  In TypeScript, let the compiler infer obvious locals and narrow engine JSON
  at the boundary (`src/game/engine-api.ts`).
- `cargo fmt` and clippy (`-D warnings`) own Rust style; Prettier owns the rest.
  The page compiles with TypeScript 7; lint uses a separate TypeScript 6
  toolchain (see `tools/lint/README.md`).

## Normal development and validation

Use Node.js 24 or newer, pnpm 11 or newer and Rust through rustup; if `pnpm`
is missing, install it with `brew install pnpm`. `rust-toolchain.toml` pins
Rust with rustfmt, clippy and the `wasm32-unknown-unknown` and
`x86_64-unknown-linux-musl` targets; the wasm-bindgen CLI must be 0.2.129, the
crate pin. Put rustup's cargo before Homebrew's:
`export PATH=/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH`. `devEngines` in `package.json` pins the exact pnpm
version, which pnpm 11+ switches to by itself, and makes npm (including `npx`)
refuse to run; `engines.pnpm` rejects older pnpm, which ignores the pin. Run
`pnpm install` in every new checkout or worktree and after dependency changes;
it replaces `npm ci`, `pnpm add` replaces `npm install <package>`, and the
root postinstall also installs the isolated lint toolchain in `tools/lint/`.
Packages come from pnpm's shared store, so a worktree install takes about a
second and no extra disk. pnpm blocks dependency install scripts: a new
dependency that needs one fails the install until `pnpm approve-builds <name>`
records it under `allowBuilds` in `pnpm-workspace.yaml`. Local Rust builds may run
through [Kache](README.md#run), a user-level `rustc-wrapper` that restores compiler
outputs across worktrees; keep it out of the repository's Cargo config and CI. If a
build looks stale, rerun it with `KACHE_DISABLED=1` before debugging the code, and say
whether Kache was on when reporting build times.

```sh
pnpm run check                        # CI gate: format, Wasm/Vite build, lint, types, clippy, server, all tests
pnpm run wasm                         # release Wasm into src/generated/engine/; rerun after Rust/WGSL edits
cargo test -p sloppy-core --test navigation   # focused Rust test (add --release for seeded matches)
node --import tsx --test tests/foo.test.ts    # focused page-shell test
pnpm run validate                     # seeded headless matches and reset checks
pnpm run dev                          # browser work; use the printed URL
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ pnpm run check:browser
```

`pnpm run validate` writes its results to the ignored
`artifacts/performance/simulation-results.json`. `pnpm run build` creates
`dist/`; it is a build output, not a place to edit source behavior. Vite and
`tsc` import the generated glue, so run `pnpm run wasm` before them; `build`,
`build:dev` and `check` do. `tests/traffic-bots.test.ts` drives the built
server binary (`pnpm run server:build`).

A successful Rust/TypeScript/build/test gate does not prove controls, menu
transitions, rendering, or cleanup. Run `pnpm run check:browser` against the
dev server for startup, menu, input or rendering changes, or the focused check
from `scripts/README.md` while iterating. Keep those checks passing: fix or
delete a check that no longer matches the game rather than leaving it broken,
and start rounds through `startRound()` in `scripts/browser-helpers.mjs`. If
the reported bug is a real pointer interaction, verify it with a physical
coordinate click (`page.mouse`); a locator or accessibility activation can
bypass pointer-event and coordinate-routing bugs. Checks launch headless Chrome
through the shared `headless` flag so they never pop windows over the user's
desktop; only `SLOPPY_HEADED=1` opens a visible window.

Phones get only a deliberately limited edition (`src/game/phone-mode.ts`: tank and
map setup, Easy single player, drive stick, touch the arena to aim and fire); do not grow it into full phone
support unless explicitly requested. Focus browser validation on desktop; retain
tablet/iPad touch support, the phone edition and their input checks.

Profiling and benchmarks (`profile.mjs`, the loading and host-download
benchmarks) are manual evidence, not normal CI. Do not add these
workloads to `pnpm run check` or deployment workflows. Keep HTTP delivery,
browser cold-load, and in-game rendering/gameplay conclusions separate; a
result from one category does not prove the others. Preserve outliers and
disclose sample counts instead of reporting a clean percentile that discarded
a slow run.

## Simulation contracts

- Gameplay advances at the fixed `STEP` of 1/60 second (`sim/data.rs`).
  Rendering may interpolate between previous and current physics poses, but
  presentation code must not move Rapier bodies or make gameplay depend on
  display Hz. `crates/render` reads `RenderState` copies, never the world.
- `Simulation::step` has an intentional order: update match time and live
  tanks/commands, advance Rapier, resolve debris/cover motion and projectile
  or mine contacts, then repair, pickups, and debris cleanup. Preserve this
  order unless the behavior change is deliberate and tested.
- Human and bot input use the same `VehicleCommand`. Continuous input may stay
  held, but one-shot actions such as mines and ammo selection are consumed by a
  simulation tick, not once per rendered frame.
- Simulation and rendering have separate lifetimes. `Simulation::reset`
  rebuilds its Rapier world and contact queue in place (the body count returns
  to the initial one; `validate` asserts it). Respawn recreates a tank body
  but keeps the tank identity, score, and lifetime rules.
- Long-running sessions must stay bounded: solo mode reuses its six enemy
  slots, fragments are capped by `MAX_FRAGMENTS`, and pending events are
  capped. Do not append replacement tanks or leave dead physics bodies,
  colliders, HUD nodes, or effect entries behind.

## Determinism and combat rules

- The seeded `Random` stream (Mulberry32 with the former double-valued state) is
  gameplay state. Its draw order is part of the
  match contract: inserting, removing, or reordering a draw can change bot
  decisions, trajectories, destruction, and outcomes for the same seed.
  Use `simulation.rng` for gameplay randomness. Cosmetic variation uses the
  renderer's `CosmeticRandom` (`crates/render/src/effects/random.rs`), which must
  never influence combat, navigation, spawning, or seeded validation. The same
  seed must give the same match natively and in Wasm;
  `crates/core/tests/initial_state.rs` and `examples/trace.rs` check construction
  and per-tick parity.
- Bot names deliberately use a separate round-derived stream
  (`shuffled_bot_names`) so they do not consume combat RNG. Keep that separation.
- Projectile contacts are continuous and resolved earliest-first across all
  shells; after a bounce, interception, or destruction, the next contact is
  queried again. Do not replace this with array order or one ray per shot per
  tick.
- Route tank and cover damage through the existing damage helpers. Other
  same-team tanks are immune to hull/shield damage, but an allied tank still
  blocks a projectile lane; self-damage remains a separate allowed case.
  Rockets retain their existing friendly-contact and self-damage behavior.
  Update both the damage path and the blocking/query path when changing this
  rule.
- Preserve `ownerLife` propagation through projectiles, mines, explosions, and
  destruction chains. Ordnance created by an old tank life must not award XP
  to a replacement tank. Mark/remove chain sources before recursing so drums,
  mines, and adjacent cover resolve exactly once.
- Destructible cover has three coupled states: visual `alive`, Rapier body or
  collider membership, and navigation occupancy. On destruction update all
  applicable states and rebuild the affected navigation region. Trees retain
  a tank-only stump footprint; do not use a visual-only fix to alter shell or
  tank collision semantics.

## Resource and rendering ownership

- Renderer resources carry a `Lifetime`: `Shared` models, cached meshes,
  materials, textures and themed scenery outlive round resets, while
  `reset_round` releases only `Round` models, instances and scenery. GPU meshes
  and materials keyed by `Arc` identity stay while another owner holds the
  `Arc`; do not free shared resources merely because an instance disappeared.
  Reuse GPU buffers on reset and destroy superseded size-dependent textures on
  resize.
- New or respawned simulation entities need their presentation instance and HUD
  bar before the next draw. Keep render-only recoil, interpolation, suspension,
  particles, tracks, and debris separate from authoritative physics state.
- Preserve batching: parts of instances sharing a mesh and material draw as one
  instanced call, identical covers share models, static scenery is baked per
  material and cell, and shadow casters merge per model. Do not turn a model
  part or an entity into its own draw or its own Wasm/JS crossing.
- Wasm linear memory never shrinks, so the page keeps its peak heap for good.
  Fill GPU buffers with `queue.write_buffer`, not `create_buffer_init` or
  `mapped_at_creation` (the browser backend stages the whole mapped range in a
  Wasm-side copy); stream large meshes in chunks; hand generated pixels to the
  GPU as JS `ImageData`; size merged geometry up front and move, not clone,
  parts a merge consumes. Check a map's `WebAssembly.Memory` size after loading
  it when changing scenery or upload paths.
- Meshes have no GPU buffers of their own: they share vertex and index pages
  (`crates/render/src/mesh_pages.rs`, `MeshStore`), their indices written absolute
  in the vertex page, so every draw passes `base_vertex` 0 (WebGL2 has none) and a
  pass rebinds only when the page changes. Bind a page only through
  `MeshStore::vertex_buffers` and `index_buffer`, which bind its written prefix,
  where every mesh lives: a `slice(..)` of a page makes wgpu clear its unwritten
  tail, on WebGL through a page-sized vector of zeros in the Wasm heap. Batch and
  own pages go with their last mesh, and a general page once a frame's collection
  finds it empty, so a reset's new round first refills the general pages the old
  one emptied. `View::reset` drops the old round's views before `reset_round`:
  cover models hold their source meshes, and a round that uploads before those
  are freed lands in new pages while the old ones empty a frame later.
- Preserve bounded pools and capacity assumptions for particles, fragments,
  tracks, effects and diagnostics. If a change adds a new per-frame allocation
  or growing collection, measure reset and long-run behavior.
- Rendering is WebGPU through `wgpu`, with direct WebGL2 through `glow` as a
  separately downloaded fallback (`--no-default-features --features webgl`,
  `src/generated/engine-webgl/`). `src/engine.ts` selects it when WebGPU has no
  adapter or device; `?webgl` forces it. Keep the WebGL dependency graph free of
  wgpu-core and wgpu-hal. Inspect it with `cargo tree --target
wasm32-unknown-unknown -p sloppy-web --no-default-features --features webgl`.
  Scene preparation, resources' CPU ownership, mesh placement, culling, draw lists,
  batching and effects stay shared in `gpu/mod.rs` and `gpu/resources.rs`.
  WebGPU submission lives in `gpu/webgpu.rs`; direct GL resources and submission
  live in `gpu/gl/`. Do not introduce a general GPU API between them.
- WGSL is authoritative for both backends. `glsl.rs` translates through Naga and
  resolves the game's fixed resource slots from reflection; never hand-maintain
  matching GLSL. WebGL instances use RGBA32F textures, Naga's first-instance
  uniform handles surface draws, and merged casters offset their instance-rate
  base attribute. Preserve absolute mesh indices and transparent ordering.
- Every GL mutation must keep `gpu/gl/device.rs`'s state cache accurate, including
  buffer/image uploads, generic uniform bindings changed by `bindBufferBase`,
  depth write masks during clears, framebuffer resolves and deletion. Superseded
  attachments and unowned objects are deleted. Naga flips clip Y and maps depth
  from 0..1 to -1..1; GL culls clockwise fronts and the final blit flips Y back for
  the canvas. Preserve linear HDR, sRGB textures, alpha, 4x MSAA and bitmap/ImageData
  orientation. Inspect console and `Game.error()` as well as screenshots. Setup,
  uploads, resize and warm-up check GL errors immediately; runtime polls every
  30 submitted frames to avoid a synchronous browser-process round trip on every
  draw. Context loss is checked immediately. Keep the polling cost in benchmarks.
  `scripts/webgl-check.mjs` covers every map, fallback, still-frame parity, GL
  object counts and Wasm memory through resets/resizes, and context loss.
- Compute derivatives (`fwidth`, `dpdx`) before non-uniform branches, and match
  sRGB formats and MSAA counts between pipelines and attachments. Native shader
  and registry tests validate WGSL and the same GLSL translator used at runtime.
- Pipelines compile on demand and stay cached; `prepare_step` and `warm_up`
  compile the arena's shadow, reflection and effect variants before the first
  gameplay frame. Validate moving cameras, first-use effects and mid-round
  destruction when changing batching, shadow merging or warm-up.
  `drawCalls` in the stats counts main-pass draws; shadow and reflection draws
  are reported separately.

## Maps, extra levels, and authored data

Map layouts are gameplay data, not only scenery. A new or moved obstacle,
pickup, or spawn must preserve hull clearance, team access, and navigation
reachability; add or update a test for those properties. Check both projectile
line-of-sight and tank steering when changing cover geometry.

Extra levels are maps with their own arena, roster and rules, offered in Battle
Setup's map dropdowns (marked EXTRA, in their own group) only on a page opened
with `?debug`. Each is a map option with `extra: true`
(`crates/core/src/sim/map_options.rs`, mirrored for the menu in
`src/game/map-options.ts`) and a setup in `sim/extra_levels.rs` (`extra_level`).
Single player plays one as an endless team battle (`single_player_rules`);
switching back applies the standard rules, so every rule a level sets needs a
standard value there. A room plays one like any map: the host picks it,
`create_multiplayer_simulation` applies the same setup with the room's round
rules, and plain `/rooms` (which the traffic bots read) leaves those rooms out
while Battle Setup asks for `/rooms?debug`. Room clients learn a level
from the replicated scene (`map.theme`, `map.scale`).

The Stress Grid (`stress-test`) is an intentional workload (30 tanks, 75
destructible objects and an 80-fragment cap). It should remain useful for
finding body, navigation, destruction, and resource-growth regressions, not be
weakened to make a normal match look healthy.

The Scrap Yard (`superstress`) packs the same 30 tanks into a yard at
`SUPERSTRESS_SCALE` of the standard arena, with over 100 destructibles, a
240-fragment budget, cover that rebuilds in place and debris that lingers until
the budget needs room. Its rules live in `sim/superstress_level.rs` and reach the
game only through `Simulation::after_step`, `restore_cover` and the map's `scale`;
keep level behaviour there rather than branching on it in shared code. Author
its placements in standard-arena coordinates so the scale stays one knob. Both
levels give players a near-invulnerable hull and boosted pickups, online and
offline.

## Assets, deployment, and evidence

- Runtime assets live in `public/`; source artwork and generator notes live
  under `assets/texture-sources/`. Regenerate checked-in WebP/audio assets
  only for an intentional artwork or sound change, and use the documented
  encoders rather than hand-editing generated binaries.
- Production is GitHub Pages at the custom domain `sloppy-tanks.fridman.me`; its
  workflow builds with `DEPLOY_BASE=/`, and `fridman.me/sloppy-tanks/` redirects
  there. Local builds and the dev server keep the default `/sloppy-tanks/` base.
  The workflow runs the same parallel jobs as the pull-request check
  (`.github/workflows/check.yml`, each running one of the scripts
  `pnpm run check` chains), so a lint/type/build/test failure blocks the
  deploy. Add a new gate step to one of those scripts, not to `check` itself,
  or CI will skip it.
- Treat historical artifacts, frame rates, CDN measurements, and deployment
  results as evidence from a particular environment and time. Re-measure live
  state before making current host or performance claims.
- Keep one-off screenshots, profiles and reports under the ignored
  `artifacts/performance/`. Update enduring documentation only for current
  behavior, workflows, invariants or asset provenance; historical measurements
  belong in local artifacts or the commit description.

## Local dev publishing

- `pnpm run deploy:dev` checks the checkout, builds `dist-dev/`, and uploads it
  to the dedicated Cloudflare Pages project `sloppy-tanks-dev` (setup and URLs
  in `README.md`). It publishes current local files, including uncommitted
  changes. Use this when asked to publish the dev site. Do not publish it to
  production or change the production GitHub Pages workflow, and never upload
  the repository directory.
- `deploy:dev` first deploys the dev site's own multiplayer server
  (`scripts/deploy-vps.mjs --dev`, key-based SSH): a second service,
  `sloppy-tanks-dev`, on the same Vultr VPS, served at port 8443 of the
  production hostname. It resets only the dev server's rooms and never touches
  production's server. Client and server must agree on a content hash of the
  `crates/core` sources, the crates they resolve to and `rust-toolchain.toml`
  (list them with `node scripts/content-version.mjs`); `crates/render`,
  `crates/web` and the page shell are outside it. The VPS runs both servers as
  container images under Podman: CI pushes one per server build and, after the
  Pages deploy, moves `:production` to main's. Production then needs
  `pnpm run server:update` unless auto-update is on (`server:status` shows it); ask before running it.
  `pnpm run server:deploy` from `main` is the SSH fallback when CI or the
  registry cannot serve. Never run `server:auto-update`, `server:rollback` or
  `server:update` for production without being asked. The dev server only changes
  on `deploy:dev` or an explicit `--dev` command.
  `pnpm run server:check-if-redeployment-required` (add `--dev` for
  the dev server) compares this checkout with a live server's `/health` and
  exits non-zero when a redeploy is needed. It tells clients-refused (content
  hash) apart from server-only changes (`serverBuild`: `crates/server`, its
  dependencies, build settings). Run it after a merge instead of judging by
  which directories changed.
  Change the VPS only through `deploy/vps/` and the `server:*` deploy
  scripts described in `crates/server/README.md`. The traffic bots in `bots/` remain
  a Cloudflare Worker that targets the VPS; `pnpm run bots:deploy` publishes
  them separately.
- Keep `dist-dev/` excluded from Git, formatting, and lint discovery.
- `scripts/dev-site.ts` is the explicit allowlist for `/test-pages.html`. Add
  suitable HTML entries there and smoke-test their deployed assets and
  behavior; asset generators are automation tools, not test pages.
- Keep dev pages free of build footers and navigation overlays. The game's Battle
  Setup shows one build line (release version and commit); build time and the rest
  belong in `/health` (`scripts/page-health.ts`, on production too).
- The release version is `MAJOR.MINOR.PATCH.BUILD` (`scripts/release-version.mjs`):
  `version` in package.json, bumped by hand, plus the Pages workflow's
  `github.run_number`, so only main's deploys have the fourth part. `/health` reports
  it as `version`, beside `protocol`, the number clients and the traffic bots read. The server image carries the release of the first main build that
  shipped its server build; later builds only retag it, so the VPS does not restart
  for commits that leave the server unchanged.
- After publishing, check the game, test directory, representative fixtures,
  and build metadata through the public URL. A successful upload is not a
  browser check. Dev responses request `noindex`; this is a public site, not
  access control.

## Temporary Cloudflare test links

Create a Cloudflare tunnel **only when the user explicitly requests one**, and
follow `docs/cloudflare-tunnel.md`. Do not create public links automatically
for development or browser checks, and never change the Pages deployments.
