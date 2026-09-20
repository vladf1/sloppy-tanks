# Sloppy Tanks agent guide

This file records the project rules that are easy to violate and expensive to
rediscover. Use `README.md` for player-facing behavior and `CONTRIBUTING.md`
for the fuller style and browser-check catalog; do not turn this file into a
second directory listing.

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

## Normal development and validation

Use Node.js 24 or newer. After dependency changes, run `npm ci`; the root
postinstall also installs the isolated lint toolchain in `tools/lint/`.

```sh
npm run check                         # CI gate: lint, format, build and tests
node --import tsx --test tests/foo.test.ts  # focused test file
npm run validate                      # seeded headless matches and reset checks
npm run dev                           # browser work; use the printed URL
```

`npm run validate` is not a passive read: it rewrites the tracked
`artifacts/simulation-results.json`. Inspect that diff and keep it only when
the validation output is intentionally part of the change. `npm run build`
creates `dist/` and copies JSON reports from `artifacts/`; these are build
outputs, not a place to edit source behavior.

For a browser regression, set the exact Vite URL explicitly when needed:

```sh
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ node scripts/browser-check.mjs
```

Use the focused script that matches the changed path (driving, startup,
ammunition, combat feedback, bot movement, destruction, or map checks). A
successful TypeScript/build/test gate does not prove controls, menu
transitions, rendering, or cleanup. If the reported bug is a real pointer
interaction, verify it with a physical coordinate click in the visible Chrome
window; a locator or accessibility activation can bypass pointer-event and
coordinate-routing bugs.

Long-running profiling and benchmarks are manual evidence, not normal CI:

- `scripts/benchmark.mjs` includes stress, reset, and a long longevity run.
- `scripts/profile.mjs before|after` is for matched runtime comparisons.
- `scripts/benchmarks/host-download-benchmark.mjs` measures HTTP resource
  delivery only and writes dated results under `scripts/benchmarks/results/`.

Do not add these workloads to `npm run check` or deployment workflows. Keep
HTTP delivery, browser cold-load, and in-game rendering/gameplay conclusions
separate; a result from one category does not prove the others. Preserve
outliers and disclose sample counts instead of reporting a clean percentile
that discarded a slow run.

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
- Map selection on `Surprise me` deliberately uses a separate round-derived
  stream so it does not consume combat RNG. Keep that separation.
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
