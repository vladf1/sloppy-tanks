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
  a Rust unit test beside the code, a module of `crates/core/tests/integration/`
  (one test binary; a new file directly under `tests/` would be another binary to
  compile and link on every engine edit) for engine and net behavior, a
  `crates/server/tests/` file for the server, `tests/*.test.ts` for the page shell.
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
- `crates/render` (Wasm): the WebGPU (`wgpu`) and WebGL2 (`glow`) renderer, WGSL shaders
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
  Bun checks the page; lint uses a separate TypeScript 6
  toolchain (see `tools/lint/README.md`).

## Normal development and validation

Use Bun 1.4.4 (pinned in `.bun-version` and `package.json`) and Rust through rustup.
Bun supplies installation, TypeScript checking, script execution and the native
test runner; Vite, running on Bun, bundles the page and serves it in development.
ESLint and Prettier run on Bun;
TypeScript 6 in `tools/lint/` exists only for ESLint's parser and typed rules.
`rust-toolchain.toml` pins Rust with rustfmt, clippy and the
`wasm32-unknown-unknown` and `x86_64-unknown-linux-musl` targets; the wasm-bindgen CLI
must be 0.2.129, the crate pin. Put rustup's cargo before Homebrew's:
`export PATH=/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH`.
Run `bun install --frozen-lockfile` in every new checkout or worktree. The root
postinstall installs `tools/lint/` with its own committed `bun.lock`; use `bun add`
for dependency changes. Installs use isolated linking; root dependencies cannot
run install scripts. Wrangler lives in `tools/deploy/` with a separate lockfile;
`bun run wrangler` and the Cloudflare deployment commands install it on demand
with Bun, then run Wrangler on Node.js 22 or newer. This is the sole Node runtime
exception while Bun's Undici dispatcher support is incomplete; do not force it onto Bun.
Only that package trusts `esbuild` and `workerd`. Review a new dependency's install
script before adding it to `trustedDependencies`. Keep `--no-script` on the native
`bun check` command: this repository also has a `check` package script.
Local Rust builds may run
through [Kache](README.md#run), a user-level `rustc-wrapper` that restores compiler
outputs across worktrees; keep it out of the repository's Cargo config and CI. If a
build looks stale, rerun it with `KACHE_DISABLED=1` before debugging the code, and say
whether Kache was on when reporting build times.

```sh
bun run check                        # CI gate: format, Wasm/Vite build, lint, types, clippy, server, all tests
bun run wasm                         # release WebGPU engine into src/generated/engine/; rerun after Rust/WGSL edits
bun run wasm:webgl                   # the WebGL2 fallback engine; before trying ?webgl after an edit
cargo test --workspace --test integration navigation::   # one area of the core tests (add --release for seeded matches)
bun test tests/foo.test.ts    # focused page-shell test
bun run validate                     # seeded headless matches and reset checks
bun run dev                          # browser work; use the printed URL
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ bun run check:browser
```

`bun run validate` writes its results to the ignored
`artifacts/performance/simulation-results.json`. `bun run build` creates
`dist/`; it is a build output, not a place to edit source behavior. Vite and
`bun check` import the generated glue, so run `bun run wasm` before them; `build`,
`build:dev` and `check` do. `bun run wasm` leaves the WebGL2 engine
(`src/generated/engine-webgl/`) alone unless its glue is missing or the engine's
API changed, because building it doubles the compile work of every edit; the
browser checks that play it rebuild it first, and `build` builds both
(`bun run wasm:all`). Keep `--workspace` on focused cargo commands: `-p sloppy-core`
alone resolves different dependency features and compiles Rapier and its stack a
second time. `tests/traffic-bots.test.ts` drives the built server binary
(`bun run server:build`).

A successful Rust/TypeScript/build/test gate does not prove controls, menu
transitions, rendering, or cleanup. Run `bun run check:browser` against the
dev server for startup, menu, input or rendering changes, or the focused check
from `scripts/README.md` while iterating. Keep those checks passing: fix or
delete a check that no longer matches the game rather than leaving it broken,
and start rounds through `startRound()` in `scripts/browser-helpers.mjs`. If
the reported bug is a real pointer interaction, verify it with a physical
coordinate click (`page.mouse`); a locator or accessibility activation can
bypass pointer-event and coordinate-routing bugs. Checks launch headless Chrome
through the shared `headless` flag so they never pop windows over the user's
desktop; only `SLOPPY_HEADED=1` opens a visible window.

Phones get only a deliberately limited edition (`src/game/phone-mode.ts`: tank, map
and battle format setup, Easy single player, a one-action multiplayer tab that joins the
busiest open room or creates one, drive stick, touch the arena to aim and fire); do not
grow it into full phone support unless explicitly requested. Focus browser validation on
desktop; retain tablet/iPad touch support, the phone edition and their input checks.

Profiling and benchmarks (`profile.mjs`, the loading and host-download
benchmarks) are manual evidence, not normal CI. Do not add these
workloads to `bun run check` or deployment workflows. Keep HTTP delivery,
browser cold-load, and in-game rendering/gameplay conclusions separate; a
result from one category does not prove the others. Preserve outliers and
disclose sample counts instead of reporting a clean percentile that discarded
a slow run.

Separate page runs of one build drift 2x on a developer Mac, so compare render
CPU (`renderMs`, the engine's presentation and `Renderer::render`) between builds
loaded at once: debug-API static bundles (`NODE_ENV=development bun x vite
build --minify false`) served cross-origin isolated for 5 µs timer resolution, one
page per build, seeded, with the overview camera; hold the game's `loop`, step
every page through the same timestamps in alternating 40-frame blocks, wait for
the GPU before each frame, pair frame _n_ across builds and report the mean paired
difference ± two standard errors over blocks. Run it again with the page order
reversed, since the first page created tends to be faster. Call counts (wrapping
the WebGPU or WebGL prototypes) and profiles (CDP profiler, which runs pages
about twice as fast) explain a result but never time one; the GPU process's CPU,
sampled with `ps` over fresh browsers, is a separate measurement.

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
  `crates/core/tests/integration/initial_state.rs` and `examples/trace.rs` check
  construction and per-tick parity.
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
  parts a merge consumes. Keep no CPU copy of a mesh the GPU already holds: the
  mesh store keeps only a `Weak`, so a model registered once (the wrecks, themed
  scenery) must not also sit in a cache or a retained tree. Check a map's
  `WebAssembly.Memory` size after loading it when changing scenery or upload paths.
- Meshes have no GPU buffers of their own: they share vertex and index pages
  (`crates/render/src/mesh_pages.rs`, `MeshStore`), their indices written absolute
  in the vertex page, so every draw passes `base_vertex` 0 (WebGL2 has none) and a
  pass rebinds only when the page changes. On WebGPU bind a page only through
  `PageBuffers::vertex_buffers` and `index_buffer` (`gpu/webgpu/resources.rs`), which
  bind its written prefix, where every mesh lives: a `slice(..)` of a page makes wgpu
  clear its unwritten tail first. On WebGL a vertex page owns its vertex array, whose
  element buffer is the index page it last drew from. Batch and
  own pages go with their last mesh, and a general page once a frame's collection
  finds it empty, so a reset's new round first refills the general pages the old
  one emptied. `Presentation::reset` drops the old round's views before `reset_round`:
  cover models hold their source meshes, and a round that uploads before those
  are freed lands in new pages while the old ones empty a frame later.
- Preserve bounded pools and capacity assumptions for particles, fragments,
  tracks, effects and diagnostics. If a change adds a new per-frame allocation
  or growing collection, measure reset and long-run behavior.
- Rendering is WebGPU (`wgpu` with `Backends::BROWSER_WEBGPU`), with a WebGL2
  fallback: a second engine build (`--no-default-features --features webgl`,
  `src/generated/engine-webgl/`) on glow, without wgpu. `src/engine.ts` loads it
  only where the browser gives no WebGPU adapter or device (`?webgl` forces it),
  so a WebGPU page never downloads WebGL code. The renderer core (`gpu/mod.rs` and
  its shared mesh, material, texture and pool stores) builds the same draw lists
  for both; the browser API is one backend module chosen at build time,
  `gpu/webgpu/` or `gpu/webgl/`, each with the same few concrete types (`Gpu`,
  `Frame`, `Pipelines`, `PageBuffers`, `MaterialBinding`, `InstanceStore`,
  `Uploader`) whose GPU objects free themselves on drop. Keep shared logic out of
  the backends and add no trait or wgpu-like layer between them. The WebGL backend
  translates the same WGSL with naga (`shader::glsl`) and sets GL state only
  through its cache (`gpu/webgl/context.rs`); uploads go through
  `COPY_WRITE_BUFFER` and the upload texture unit so they never disturb what draws
  bound. WebGL2's gaps: instance records live in an RGBA32F texture rather than a
  storage buffer (`instances_texture.wgsl`), the first instance is a per-program
  uniform, programs bind their blocks and samplers by name to fixed points and
  units, the cached fixed-scenery shadow is copied with a depth blit, bitmaps are
  flipped at decode, and the output pass flips rows into the bottom-up canvas.
  A WebGL draw binds only the state that differs from the draw before it, and no
  material when its program reads none. `getError` waits for the GPU process, so
  it runs every few seconds, before a frame's draws. Both backends group opaque
  draws by pipeline and then by what their bindings cost (`DRAW_GROUPING`:
  material first on WebGL, mesh page first on WebGPU), so depth ties can resolve
  differently between the engines.
  WebGL may simplify an effect, but must not give up a performance optimization
  such as a cache or batching: its devices are the weaker ones. Keep both
  building: `bun run rust:clippy` lints both, `scripts/webgl-check.mjs` plays the
  fallback and `SLOPPY_WEBGL=1` runs any browser check on the WebGL engine (both
  rebuild it first through `buildWebglEngine()` in `scripts/browser-helpers.mjs`).
  Shaders are handwritten WGSL; custom model effects register
  an `EffectDefinition`. The native `shader`/`registry` tests validate every
  variant with naga and translate it to WebGL's GLSL ES 3.00, but a browser can
  still reject a pipeline: inspect console
  and GPU errors (`Game.error()`) as well as screenshots. Compute derivatives
  (`fwidth`, `dpdx`) before non-uniform branches, and match sRGB formats and
  MSAA counts between pipelines and attachments.
- Main-thread render CPU decides frame drops on slow devices; desktops hold 60 Hz
  either way. Chrome's WebGPU `writeBuffer` costs about 0.35 µs per KB on the main
  thread whatever the call count, so keep per-frame records small (`InstanceRecord`
  is 80 bytes) and draw only what a view can show (the reflection culls to the
  cells its water samples, `reflection_cull.rs`). Fewer calls is not less CPU by
  itself: grouping WebGPU draws by state removed about 1,000 calls a village frame
  and saved only the GPU process, and `WEBGL_multi_draw` saved nothing, so time a
  change rather than count its calls. `sloppy-render` builds at opt-level 3 for
  speed; the rest of the engine favors size.
- On big canvases (a 4K window at the 1.5 pixel ratio) per-pixel work decides the
  GPU frame: the ground shader runs on most pixels and smoke puffs overlap many
  layers deep. Run a procedural feature only where its weight is non-zero, and
  noise and lighting only where a puff has density (an `if` whose skipped pixels
  get exactly the value they would have computed, with texture samples and
  derivatives ahead of it); the village ground and burning wrecks once cost 6 and
  up to 13 ms a 4K frame. Time a ground, full-screen or layered effect at
  3840×2160 with WebGPU timestamps, switching it off within the page
  (`debug_set_off`), before keeping it. A ground's still layers (patches, soil
  colour, slopes, anything that neither moves nor depends on the view) belong in a
  texture bake (`effects::bakes`: a generated texture a full-screen pass draws once,
  from its uv alone or from a material's params and textures), sampled per pixel;
  the village ground's per-pixel noise once cost 4 ms of an iPad frame in WebKit.
- The scene renders at a scale of the canvas that the quality controller
  (`crates/render/src/quality.rs`, the Graphics setting) picks, and the output pass
  upsamples it; at scale 1 the output reads texels exactly as before, so the High
  tier on a desktop is pixel for pixel today's picture. Tiers switch optional work
  through uniforms and passes (`TierFeatures`), never through pipeline variants.
  Touch devices also draw without MSAA, which is fixed per renderer. Keep a new
  expensive per-pixel feature switchable by a tier. WebKit lowers a WebGPU canvas
  to 15-30 fps after a single frame overruns, and builds from August to October 2026
  (bug 325095, Playwright's WebKit too) never raise it again on their own; the
  controller reconfigures the canvas when frames stay held while the GPU has room, so
  do not remove `reset_canvas_pacing` as dead code. Keep that room measured against a
  display frame: those builds judge a reset canvas on its first frames at the GPU's
  clock of the moment, and an Apple GPU clocks down while frames are held (5-6 ms of
  village work spanned 21-25 ms at 30 fps), so resetting then drops the canvas lower.
  Measure with Graphics pinned (localStorage `sloppy-graphics`) or
  `debug_set_scene_load`, since Auto otherwise changes what is being timed.
- WebKit (Safari, every iPad) guards each WGSL loop with a volatile
  forward-progress counter, so Metal cannot unroll it and arrays or vector lanes
  indexed by the loop counter spill to memory: per-pixel loops ran 15-45x slower
  than in Chrome (a village frame took 80 ms of GPU time against 4.7). The shader
  specializer unrolls every literal- or const-bounded loop and a test fails if a
  variant keeps one; write new loops that way. Chrome timings hide this, so time a
  new or grown shader in Playwright WebKit at an iPad size too.
- WebKit's cold start (an empty Metal shader cache, as after any shader change) is
  all compiling, one pipeline at a time even through `createRenderPipelineAsync`:
  a Metal library for every stage of every pipeline (about 0.1 s each, and 45 ms
  more if the layout shows that stage any texture), then a machine-code compile
  at each fragment stage's first draw per blend state. Metal caches libraries by
  their text, so each stage is a module with only its own code (`StageSources`)
  and textures are visible to the fragment stage alone. Count a new variant or
  effect as its stages' compiles, and time startup with Playwright WebKit's Metal
  cache cleared (`com.apple.metal` under `$(getconf DARWIN_USER_CACHE_DIR)`
  `com.apple.WebKit.GPU+org.webkit.Playwright/com.apple.WebKit.GPU/`).
- Pipelines compile on demand and stay cached; `prepare_step` and `warm_up`
  compile the arena's shadow, reflection and effect variants before the first
  gameplay frame. Validate moving cameras, first-use effects and mid-round
  destruction when changing batching, shadow merging or warm-up.
  `drawCalls` in the stats counts main-pass draws; shadow and reflection draws
  are reported separately.

## Camera and effect feel

These come from playing the game and are easy to undo with "more juice":

- The camera shakes only when the viewer is hit or destroyed, and never rolls.
  The player's own shots, kills, other tanks dying and explosions leave it
  still: no shake, tilt, kick, zoom punch or pull toward the blast (they gave
  the player vertigo). Tanks themselves may rock and recoil.
- The overhead camera sits at a fixed offset on the player's tank (about 52
  degrees, default zoom 34) and looks straight at it: no lean toward the
  pointer, the drive or enemies, no lagging springs and no automatic zoom. A
  lower, closer or wandering camera hid the field, made every difficulty play
  harder and distracted the player.
- The action never pauses or slows: no hit-stop or slow motion on kills or hits.
  It read as dropped frames.
- Effects follow what their object is doing: a wreck burns only once its hull
  rests, smoke rises and drifts without trembling, and nothing reads as a flat
  cut-out card. Judge effects frame to frame, not from one still.
- Shader animation never multiplies the running clock by a rate that varies
  per pixel, per instance or over a lifetime: the clock is large, so any change
  in the rate jumps the pattern every frame (cooling smoke flickered this way).
  Use one rate and vary the phase.
- Everything on the field belongs on a tank battlefield. Power-ups are military
  supply: steel ammo cans, wooden or steel supply crates, jerry cans, tool
  chests, worn paint and stencilled markings, with the kind's colour as a
  painted band or a wisp of signal smoke. No science fiction: no holograms,
  neon panels, light beams or pillars, glowing pads or energy rings (they
  looked like "some science fiction movie or game").
- The kill feed is the kill confirmation. No banners or pop-ups over the
  field and no hit markers at the aim: an "ELIMINATED" banner under the score
  covered the fight on desktop, and ticks that turned red on a kill read as a
  broken cursor.

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
rules, and plain `/rooms` (which the traffic bots read) leaves those rooms out,
while the server directory lists every room and Battle Setup shows them only on a
`?debug` page. Room clients learn a level
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
  `bun run check` chains), so a lint/type/build/test failure blocks the
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

- `bun run deploy:dev` checks the checkout, builds `dist-dev/`, and publishes it
  to the dedicated Cloudflare Worker `sloppy-tanks-dev` (setup and URLs in
  `README.md`). It publishes current local files, including uncommitted
  changes. Use this when asked to publish the dev site. Do not publish it to
  production or change the production GitHub Pages workflow, and never upload
  the repository directory.
- `deploy:dev` first deploys the dev site's own multiplayer server
  (`scripts/server.mjs deploy --dev`, key-based SSH), which runs on its own
  Vultr machines (the `server:*` commands run on every machine the server directory
  lists for the environment, or on one named with `--ip`). It resets only the
  dev server's rooms and never touches production's server. Client and server
  must agree on a content hash of the `crates/core` sources, the crates they
  resolve to and `rust-toolchain.toml` (list them with
  `bun scripts/content-version.mjs`); `crates/render`, `crates/web` and the page
  shell are outside it. Each machine runs its server as a container image under
  Podman, behind Caddy's own container: CI pushes one image per server build and,
  after the Pages deploy, moves `:production` to main's. Production then needs
  `bun run server:update` unless auto-update is on (`server:status` shows it); ask before running it.
  `bun run server:deploy` from `main` is the SSH fallback when CI or the
  registry cannot serve. Never run `server:auto-update`, `server:rollback` or
  `server:update` for production without being asked. The dev server only changes
  on `deploy:dev` or an explicit `--dev` command.
- Pages find servers through the server directory, a Cloudflare Worker in
  `directory/` (`bun run directory:deploy`) that every server pushes its rooms to;
  one deployment serves production and dev by its `environment` parameter, and
  `server:provision` writes a machine's `DIRECTORY_URL` and `SERVER_ENVIRONMENT` into
  its unit.
  `bun run server:check-if-redeployment-required` (add `--dev` for
  the dev server) compares this checkout with a live server's `/health` and
  exits non-zero when a redeploy is needed. It tells clients-refused (content
  hash) apart from server-only changes (`serverBuild`: `crates/server`, its
  dependencies, build settings). Run it after a merge instead of judging by
  which directories changed.
  Change the machines only through the Vultr startup scripts
  (`deploy/server/cloud-init/`), `deploy/server/` and the `server:*` scripts described
  in `crates/server/README.md`. The traffic bots in `bots/` are a Cloudflare Worker that
  plays on the production servers the directory lists;
  `bun run bots:deploy` publishes them separately.
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
for development or browser checks, and never change the production or dev site
deployments.
