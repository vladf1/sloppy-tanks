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
  a focused regression test when the behavior is testable without a browser.
- Establish a reproducible failure or a matched before/after measurement
  before doing a broad refactor or performance change.
- Keep tests fast and deterministic. Prefer fixed-step loops, scoped random
  mocks, and an in-memory clock over real sleeps or deleting meaningful
  coverage because a visual effect is flaky.

## Code style

- Code should be easy to trace from a player action to its simulation result
  and visible feedback. Prefer descriptive domain names (`tank`, `simulation`,
  `command`, `brain`), small single-purpose functions, and explicit data over
  inheritance or abstractions that merely forward calls. Short coordinates,
  loop indices and conventional math names are fine inside small calculations.
- Name balance values, timeouts, capacities and tolerances. Shared combat rules
  live in `combat-rules.ts`, physics/lifecycle settings in `simulation-rules.ts`,
  camera and feedback timing in `view-settings.ts`; settings used by one
  algorithm stay beside it. Geometry, palettes, authored map placements and test
  expectations are data: keep them in their model, layout or fixture.
- Units are metres, seconds and radians unless a name says otherwise; DOM and
  performance timers use milliseconds. X/Z is the playable plane and Y is up.
  `alpha` is the interpolation fraction between previous and current poses.
- Comments explain intent and invariants (why a query is ordered, why a
  resource is shared), not what an assignment does.
- Use plain functions for stateless calculations and factories, and classes for
  systems that own persistent state (simulation, rendering, input, effects).
  Let TypeScript infer obvious locals; annotate contracts and boundaries, and
  narrow third-party values instead of spreading `any`.
- The game compiles with TypeScript 7; lint uses a separate TypeScript 6
  toolchain (see `tools/lint/README.md`). Prettier owns formatting.

## Normal development and validation

Use Node.js 24 or newer. After dependency changes, run `npm ci`; the root
postinstall also installs the isolated lint toolchain in `tools/lint/`.

```sh
npm run check                         # CI gate: lint, format, build and tests
node --import tsx --test tests/foo.test.ts  # focused test file
npm run validate                      # seeded headless matches and reset checks
npm run dev                           # browser work; use the printed URL
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ npm run check:browser
```

`npm run validate` is not a passive read: it rewrites the tracked
`artifacts/simulation-results.json`. Inspect that diff and keep it only when
the validation output is intentionally part of the change. `npm run build`
creates `dist/` and copies JSON reports from `artifacts/`; these are build
outputs, not a place to edit source behavior.

A successful TypeScript/build/test gate does not prove controls, menu
transitions, rendering, or cleanup. Run `npm run check:browser` against the
dev server for startup, menu, input or rendering changes, or the focused check
from `scripts/README.md` while iterating. Keep those checks passing: fix or
delete a check that no longer matches the game rather than leaving it broken,
and start rounds through `startRound()` in `scripts/browser-helpers.mjs`. If
the reported bug is a real pointer interaction, verify it with a physical
coordinate click (`page.mouse`); a locator or accessibility activation can
bypass pointer-event and coordinate-routing bugs. Checks launch headless Chrome
through the shared `headless` flag so they never pop windows over the user's
desktop; only `SLOPPY_HEADED=1` opens a visible window.

Mobile phone support is out of scope. Do not add phone-specific layouts or run
phone viewport checks unless explicitly requested. Focus browser validation on
desktop; retain existing tablet/iPad touch support and its input checks.

Profiling and benchmarks (`profile.mjs`, the loading and host-download
benchmarks) are manual evidence, not normal CI. Do not add these
workloads to `npm run check` or deployment workflows. Keep HTTP delivery,
browser cold-load, and in-game rendering/gameplay conclusions separate; a
result from one category does not prove the others. Preserve outliers and
disclose sample counts instead of reporting a clean percentile that discarded
a slow run.

## Simulation contracts

- Gameplay advances at the fixed `STEP` of 1/60 second. Rendering may
  interpolate between previous and current physics poses, but presentation
  code must not move Rapier bodies or make gameplay depend on display Hz.
- `Simulation.step` has an intentional order: update match time and live
  tanks/commands, advance Rapier, resolve debris/cover motion and projectile
  or mine contacts, then repair, pickups, and debris cleanup. Preserve this
  order unless the behavior change is deliberate and tested.
- Human and bot input use the same `VehicleCommand`. Continuous input may stay
  held, but one-shot actions such as mines and ammo selection are consumed by a
  simulation tick, not once per rendered frame.
- Simulation and rendering have separate lifetimes. `Simulation.reset()`
  frees and rebuilds its Rapier world and contact queue; headless tests and
  scripts must call `dispose()` when finished. Respawn recreates a tank body
  but keeps the tank identity, score, and lifetime rules.
- Long-running sessions must stay bounded: solo mode reuses its six enemy
  slots, fragments are capped by `MAX_FRAGMENTS`, and pending events are
  capped. Do not append replacement tanks or leave dead physics bodies,
  colliders, HUD nodes, or effect entries behind.

## Determinism and combat rules

- The seeded `Random` stream is gameplay state. Its draw order is part of the
  match contract: inserting, removing, or reordering a draw can change bot
  decisions, trajectories, destruction, and outcomes for the same seed.
  Use `simulation.rng` for gameplay randomness. Cosmetic variation may use
  `Math.random`, but it must never influence combat, navigation, spawning, or
  seeded validation.
- Bot names deliberately use a separate round-derived stream so they do not
  consume combat RNG. Keep that separation.
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

- Cached geometry, materials, scenery, and shared tank resources intentionally
  outlive round resets. `Presentation.reset()` should dispose only resources
  marked `userData.owned`; do not dispose shared resources merely because an
  instance disappeared.
- New or respawned simulation entities need their presentation model and HUD
  bar before the next draw. Keep render-only recoil, interpolation, particles,
  tracks, and debris separate from authoritative physics state.
- Preserve bounded pools and capacity assumptions for particles, fragments,
  tracks, and diagnostics. If a change adds a new per-frame allocation or
  persistent listener, measure reset and long-run behavior rather than assuming
  the browser will collect it.
- Rendering is WebGPU-only; there is no WebGL fallback. Create presentation
  with `await Presentation.create(canvas)`, write custom materials in Three.js
  TSL, and avoid `ShaderMaterial`, `onBeforeCompile`, and direct WebGL context
  access, including in fixtures and preview tools. `Presentation` delegates
  visual work to named stages; static scene creation belongs in scenery and
  model builders.
- Workarounds for the pinned Three.js release are commented with `r185`
  (renderer, render bundles, batching, effect pools). Review each one when
  upgrading Three, and inspect shader errors as well as screenshots. Validate
  moving cameras, first-use effects, and mid-round destruction when changing
  batching, bundles, or startup warm-up. `renderer.info.render.drawCalls`
  counts draws; `calls` counts renderer invocations.

## Maps, stress mode, and authored data

Map layouts are gameplay data, not only scenery. A new or moved obstacle,
pickup, or spawn must preserve hull clearance, team access, and navigation
reachability; add or update a test for those properties. Check both projectile
line-of-sight and tank steering when changing cover geometry.

`stresstest.html` is an intentional workload (30 tanks, 75 destructible
objects, endless scoring, and an 80-fragment cap). It should remain useful for
finding body, navigation, destruction, and resource-growth regressions, not
be weakened to make a normal match look healthy.

## Assets, deployment, and evidence

- Runtime assets live in `public/`; source artwork and generator notes live
  under `assets/texture-sources/`. Regenerate checked-in WebP/audio assets
  only for an intentional artwork or sound change, and use the documented
  encoders rather than hand-editing generated binaries.
- The GitHub Pages build uses the default `/sloppy-tanks/` base; the Cloudflare
  build uses `DEPLOY_BASE=/` and `dist-cloudflare/`. Keep those bases and the
  separate outputs intact. Both deployment workflows run `npm run check`, so a
  shared lint/type/build failure can break both providers.
- Treat historical artifacts, frame rates, CDN measurements, and deployment
  results as evidence from a particular environment and time. Re-measure live
  state before making current host or performance claims.
- Keep one-off screenshots, profiles and reports under the ignored
  `artifacts/performance/`. Update enduring documentation only for current
  behavior, workflows, invariants or asset provenance; historical measurements
  belong in local artifacts or the commit description.

## Local dev publishing

- `npm run deploy:dev` checks the checkout, builds `dist-dev/`, and uploads it
  to the dedicated Cloudflare Pages project `sloppy-tanks-dev` (setup and URLs
  in `README.md`). It publishes current local files, including uncommitted
  changes. Use this when asked to publish the dev site. Do not substitute the
  production project `sloppy-tanks` or change either production deployment
  workflow, and never upload the repository directory.
- Keep `dist-dev/` excluded from Git, formatting, and lint discovery.
- `scripts/dev-site.ts` is the explicit allowlist for `/test-pages.html`. Add
  suitable HTML entries there and smoke-test their deployed assets and
  behavior; asset generators are automation tools, not test pages.
- Keep dev pages free of build footers and navigation overlays; build time,
  commit, and local-change state belong in `/build-info.json` only.
- After publishing, check the game, test directory, representative fixtures,
  and build metadata through the public URL. A successful upload is not a
  browser check. Dev responses request `noindex`; this is a public site, not
  access control.

## Temporary Cloudflare test links

Create a Cloudflare tunnel **only when the user explicitly requests one**, and
follow `docs/cloudflare-tunnel.md`. Do not create public links automatically
for development or browser checks, and never change the Pages deployments.
