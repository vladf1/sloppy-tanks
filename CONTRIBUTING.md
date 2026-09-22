# Working on Sloppy Tanks

The code should be easy to trace from a player action to its simulation result and visible feedback. Prefer descriptive names, small functions with one responsibility, and explicit data over inheritance or new abstractions that merely forward calls.

## Local checks

Use Node 24 or newer, run `npm ci`, and then:

```sh
npm run check         # the same quality gate used by CI
npm run lint:fix      # safe ESLint fixes
npm run format       # format source, tests, scripts, CSS, HTML and documentation
npm run typecheck    # TypeScript 7, including tests and TypeScript tools
npm run validate     # ten full seeded headless matches plus reset checks
```

`npm run dev` prints the actual local URL. Browser regressions use an isolated Google Chrome profile. Pass that URL explicitly when Vite chooses a different port:

```sh
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ node scripts/browser-check.mjs
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ node scripts/ammunition-check.mjs
SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ node scripts/combat-feedback-check.mjs
```

A successful build does not verify controls, menu transitions, resource cleanup or the appearance of effects. Exercise those paths in Chrome when changing them. Keep generated screenshots and profiles in the ignored `artifacts/performance/` directory. Regenerate artwork or sound only when intentionally changing those assets.

## TypeScript and linting

The game compiler stays pinned to **TypeScript 7.0.2**. ESLint uses the recommended JavaScript and TypeScript rules, with type-aware checks for production TypeScript: unsafe assignments/calls, floating promises, incorrect async callbacks, unused symbols and type imports. Braces and separate variable declarations keep control flow readable. Prettier owns formatting. The compiler additionally checks strict types, unused declarations, implicit returns, switch fallthrough and overrides.

TypeScript ESLint 8.70 currently supports compiler APIs below TypeScript 6.1. Its isolated package in `tools/lint/` pins TypeScript 6.0.3 for parsing/type-aware lint analysis only; it does not compile the game. The root `postinstall` installs that package from its own lockfile. This avoids unsupported peer overrides and preserves TypeScript 7 builds. Both lockfiles belong in version control. If future TypeScript syntax is rejected by the linter, update the lint toolchain deliberately rather than suppressing its compatibility checks. See the [official typed-linting documentation](https://typescript-eslint.io/getting-started/typed-linting/).

## Readability conventions

- Use domain names such as `tank`, `simulation`, `command`, `position` and `brain`. Short coordinates (`x`, `z`, `dx`, `dz`), loop indices and conventional math terms are appropriate within small geometric calculations.
- Name balance values, timeouts, capacity limits and numerical tolerances. Shared combat rules live in `combat-rules.ts`; physics/lifecycle settings in `simulation-rules.ts`; camera and feedback timing in `view-settings.ts`. Keep settings used by one algorithm beside that algorithm.
- Distances are world metres, durations are seconds and angles are radians unless a name says otherwise. DOM/performance timers use milliseconds. X/Z is the playable plane; Y is vertical. `alpha` is the interpolation fraction between the previous and current physics poses.
- Geometry coordinates, palette colors, authored map placements and test expectations are data. Keep them in the relevant model, layout or fixture with useful assembly comments; do not turn every vertex or expected value into an unrelated global constant.
- Explain intent and invariants in comments: why a collision query is ordered, why a resource is shared, or why a stale projectile must not award XP to a new life. Avoid comments that restate an assignment.
- Use plain functions for stateless calculations and factories. Use classes when a system owns persistent state, such as simulation, rendering, input or particle effects. Do not introduce a class hierarchy for entities that are already clear typed records.
- Let TypeScript infer obvious local results. Annotate contracts and meaningful boundaries, and narrow third-party values instead of spreading `any`. Typed mesh/HUD metadata belongs beside the code that creates it.

## Invariants to preserve

`Simulation.step` controls tick order: update live tanks and commands, advance Rapier, resolve projectile/mine contacts, then repair, pickups and debris cleanup. Human and bot input use the same `VehicleCommand`. One-shot inputs are consumed by a simulation tick, not by a rendered frame.

Projectile contacts resolve earliest-first across all shells. A bounce or destruction changes the next query. Mine/drum chains remove or mark their source before recursion; they retain the initiating owner's life identifier for experience credit.

Gameplay uses the seeded `Random` stream. Reordering its draws changes a match even if the distributions look equivalent. Cosmetic particles may use `Math.random`; they cannot influence combat. Refactors should retain seeded trajectories and events unless a behavior change is intentional.

Rendering interpolates poses without moving physics bodies. New or respawned entities need their models and health bars before drawing. `Presentation` delegates visual work to named stages; static scene creation belongs in scenery/model builders.

Create presentation with `await Presentation.create(canvas)` so the native WebGPU backend initializes before scene resources are built. Custom materials use Three.js TSL. `GameRenderer` constructs the native backend directly; there is no WebGL fallback. Use it in rendering fixtures and preview tools too. Avoid `ShaderMaterial`, `onBeforeCompile`, and direct WebGL context access. Inspect shader errors as well as screenshots. `renderer.info.render.drawCalls` counts draws; `calls` counts renderer invocations. Browser scripts that control the game clock must leave Three.js's own animation callbacks running.

Opaque tank and cover parts share one native storage buffer of GPU poses and cached render bundles. Their original hierarchy still owns transforms and visibility; presentation uploads those poses once before all render passes. Rebuild batches and invalidate the bundle when models or their geometry/materials change. Source meshes have their draw layers suppressed while batched, so detached clones must call `restoreBatchedLayers`. Batch cleanup owns only the merged geometry, copied materials, and shared pose buffer, never the original model resources.

Rubble created by a falling tower stays outside the persistent pose batches, like changing timber/cargo geometry. Adding it must not rebuild every tank/tree batch and its shaders during combat. Startup waits for textures and prepares bundles, empty effect pools, shadows, reflections, and a hidden first frame before enabling gameplay. Warm-up restores visibility/count/culling state and never advances the simulation. GPU pipeline compilation overlaps node building; `GameRenderer.compileAsync()` still waits for all queued pipelines before resolving. Recheck both first-use effects and mid-round destruction when changing this path.

Startup passes the selected options directly to `Simulation` and explicitly retains round 3, matching the previous constructor-plus-reset path and its Surprise-me map seed. Default headless construction still starts at round 2. Keep both RNG state and subsequent round selection identical when changing initialization; the stress setup uses the same configuration as the existing stress fixture.

Pickup faces share one material and the preloaded `textures/pickups/atlas.webp`. Per-kind cached geometry selects each tile through UVs. Regenerate with `npm run generate:pickup-atlas` after editing the individual source icons; the ammo/full-texture commands also rebuild it. The packer uses the existing lossless WebP encoder and 16-pixel extruded gutters, preserving the artwork and protecting filtered edges. Keep the individual icons for offline generation/reference checks; gameplay requests only the atlas. Scene preloads in `src/main.ts` must match TextureLoader's URL and anonymous CORS mode so the download is reused.

`GameRenderer` contains compatibility fixes for Three r185's shadow cache, per-object binding cleanup, interleaved-buffer accounting, and cached-bundle draw statistics. Bundles execute before subsequent transparent draws; r185's deferred execution otherwise draws opaque tanks over health bars and effects. Executing a bundle also invalidates the pass binding cache. Nested shadow/reflection recording must restore the parent bundle so all its draws retain camera-update records. Validate moving cameras as well as still views when changing this path. Failed initialization releases the native backend directly: r185's renderer disposal would otherwise retry initialization. When removing models, release their renderer bindings as well as disposing owned resources. Recheck these hooks when upgrading Three. The general browser check measures actual WebGPU buffer allocations across destructive resets; stable geometry counts alone do not establish stable GPU memory.

Persistent effect pools use `storageInstances` and mark changed ranges with `needsUpdate`. Keep these buffers version-gated: r185's `DynamicDrawUsage` forces another upload for every consuming material/pass, even when the version is unchanged. Empty pools are skipped before shader processing; populate them before rendering, not from an `onBeforeRender` callback. Flag positions and their original triangle-averaged normals are calculated in the vertex shader, with fixed conservative bounds.

Cached geometry and materials outlive round resets. Only per-instance resources marked `userData.owned` are disposed by round cleanup. `isMesh` retains concrete Three.js field types after an `instanceof` check. Keep bounded capacities for particles, physics fragments, track marks and diagnostics.

## Browser and performance checks

Use the URL printed by the running Vite server for `SLOPPY_URL`. Browser scripts use installed Google Chrome with isolated profiles. Select checks relevant to the change:

`node scripts/render-bundles-check.mjs` compares cached draws against ordinary draws after moving and switching cameras on all three maps. It also checks that every bundled draw keeps an update record across nested shadow/reflection passes. Screenshots and pixel differences go to `artifacts/performance/bundle-rendering/` (or `SLOPPY_ARTIFACT_DIR`). Run it when changing render bundles, camera uniforms, or render-pass ordering.

`node scripts/pickup-atlas-check.mjs` verifies that startup requests only one pickup texture and all nine kinds share a face material. It compares near/distant renders against the separate source textures, saving screenshots and pixel differences under `artifacts/performance/pickup-atlas/`.

| Area                                         | Script or local browser page                                                |
| -------------------------------------------- | --------------------------------------------------------------------------- |
| General keyboard/mouse play                  | `scripts/browser-check.mjs`                                                 |
| Early menu, background preparation and retry | `scripts/startup-check.mjs`                                                 |
| Ammunition input and crate/HUD visuals       | `scripts/ammunition-check.mjs` (`--visual-only` skips its performance pass) |
| Touch controls and simultaneous fingers      | `scripts/touch-controls-check.mjs`                                          |
| Deferred touch loading and desktop overhead  | `scripts/touch-loading-check.mjs` (use a locally served production build)   |
| Driving controls                             | `scripts/driving-check.mjs`                                                 |
| Bot retreat and head-on movement             | `scripts/bot-movement-browser.mjs`                                          |
| Reload, hit and repair feedback              | `scripts/combat-feedback-check.mjs`                                         |
| Difficulty and combat HUD fixtures           | `/sloppy-tanks/tests/usability.browser.html`                                |
| Harbor scenery and resource reuse            | `/sloppy-tanks/tests/harbor.browser.html`                                   |
| Quarry layout, rock and barrier previews     | `/sloppy-tanks/tests/quarry.browser.html`                                   |

For a loading comparison, save each production `dist` under `artifacts/performance/loading/<label>/`, then run `npm run benchmark:loading -- <label>`. The default is five cold-cache Chrome runs at 10 Mbps / 50 ms through a local gzip server. Compare first visible content, menu appearance, final download and main-thread blocking separately. Keep each baseline and candidate snapshot unchanged during measurement.

For runtime comparisons, run `node scripts/profile.mjs before` on the baseline and `node scripts/profile.mjs after` on the changed revision, with Vite running and `SLOPPY_URL` set. Detailed results and CPU profiles stay in `artifacts/performance/`; the compact comparison is written to `artifacts/performance-results.json` after both passes complete. Avoid unrelated browser workloads while measuring.

`node scripts/frame-pacing-check.mjs` measures the first gameplay frame and seeded combat on every map without discarding a warm-up interval. Set `SLOPPY_PACING_SECONDS`, `SLOPPY_MAX_FRAME_MS`, and `SLOPPY_ARTIFACT_DIR` to control the duration, failure threshold, and report directory. It is a manual regression check; keep it out of CI and run baseline/candidate browser workloads sequentially. The startup check separately holds back both WASM and GPU compilation to verify an early coordinate GO click and late map choices.

`node scripts/benchmark.mjs` runs normal/stress workloads, ten rendered resets and a twenty-minute active-play longevity check, writing `artifacts/benchmark-results.json`. Focus pauses extend elapsed time. `npm run validate` writes accelerated simulation results to `artifacts/simulation-results.json`; those are not browser FPS measurements. The notebook at `/sloppy-tanks/benchmark.html` displays saved results.

For manual CPU profiling, open `/sloppy-tanks/tools/profile.html` in an isolated Chrome instance with remote debugging on port 9227, then run `node scripts/capture-cpu-profile.mjs LABEL 20`. The helper captures the CPU profile; use the browser UI for navigation and input.

Keep one-off reports, screenshots and raw profiles under ignored `artifacts/performance/`. Update enduring documentation only for current behavior, workflows, invariants or asset provenance; historical measurements belong in local artifacts or the commit description. CPU submission times are not GPU timings, and local frame rates are not guarantees for other devices.
