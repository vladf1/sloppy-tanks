# Validation record

## Latest movement and breakup checks

27 tests cover both breakup variants across 12 seeds, wide hull/turret separation, high arcs, landing, cleanup, scoring, respawn and the shared debris cap. `scripts/wreck-check.mjs` verifies one-click selection for all three classes, whole-number km/h labels, real rendered airborne assemblies, on-screen landings and expiry. `artifacts/wreck-check.json` stores the observations and browser errors; screenshots are `artifacts/wreck-flight.png` and `artifacts/wreck-landed.png`. The browser landing scenario clears cover to measure the trajectories; ordinary cover collisions can alter them. Previous full-match and timing records below predate this update.

## Pine Village and reference tank pass

Current checks: 27 tests, production build, and Chrome input/render verification. The browser checks verify all three tank silhouettes, exact blue/red projectile instance colors, the outlined reticle, screen centering at all four corners and five stable resets (73 bodies and 571 geometries each). Village cover tests exercise destruction and navigation updates, and all spawn slots reach midfield and both outer flanks. Screenshots are `artifacts/pine-village.png`, `artifacts/redesign-models.png` and `artifacts/redesign-aim.png`. The latest ten-match smoke test is recorded in `artifacts/simulation-results.json`; all older prose measurements below are historical. This pass does not rerun the long render benchmark.

## Latest tank, camera and bot tuning

26 tests pass, including slower bot cadence against cover while human weapon timing stays unchanged. Browser verification covers all tank models and shell styles, actual input, five stable resets, and exact screen centering at the arena center and all four corners using an interpolated moving pose. Shells use a 0.1575-unit visual radius, 50% larger than the immediately preceding small-shell revision. Existing longer-match and timing records below predate the bot tuning.

## Current arena and readability update — September 5, 2026

The arena is 120 × 120 units, with saturated blue/red tanks, a player ring, more detailed tank silhouettes, larger high-contrast shells, and reduced varied debris. Shell speeds and the shake-free camera are retained.

- `npm test`: 25 tests pass, including routes from both spawn lines to both outer flanks, driving through opened tower rubble, debris expiry and shared cap enforcement. Tower centers align with the navigation grid so their narrow cleared passages remain traversable. The traversal test now deliberately places a scout and useful pickup across the opening instead of relying on a random patrol passing through.
- `npm run validate`: ten complete seeded matches and ten body-count resets; blue won three and red seven. Eight rounds ended at the five-minute timer, two at the score limit. Both towers were destroyed in every round. This is a small balance smoke check, not evidence of statistical fairness. Results: `artifacts/simulation-results.json`.
- `caffeinate -di node scripts/redesign-check.mjs`: actual keyboard movement, held firing, pause/resume, visual inspection of spawn/menu, all three models, five shell styles and four fragment shapes. Five rendered resets each returned 65 physics bodies and 360 geometries. No runtime or console errors. Results: `artifacts/redesign-check.json`; screenshots: `artifacts/redesign-*.png`. The model gallery uses controlled placement for inspection.
- `npm run build`: type-check and production bundle pass. Vite still reports the existing large-bundle advisory.

The timing, full-input rounds and long soak recorded below belong to the preceding smaller-arena build. They were not rerun for this visual/layout update. The performance notebook labels them as historical.

## Previous build record

The machine used for browser measurements is an Apple M3 Max with 16 CPU cores, a 40-core GPU and 48 GB RAM. Browser tests use installed Google Chrome in temporary, isolated profiles. The requested render buffer is exactly 2560 × 1440 pixels at device pixel ratio 1.

## Reproducible checks

- `npm test`: 23 passing tests for damage, team safety, self-kills, attribution, protection, respawn, pickup replacement/expiry/refresh, swept hits, ricochets, mine arming and recursion, drum chains, destruction, round rules, reset, seeded randomness, idle-human bot combat, opened-route traversal, mirrored arena/spawns/pickups, fragment filtering and the shared wreck/debris hard cap. Two input regressions verify quick mine clicks and clearing controls on focus loss.
- `npm run validate`: ten complete deterministic matches, including three with idle human controls. Each match is followed by a full reset and body-count check. The archived `public/simulation-results.json` contains this previous build’s seeds, scores and timings; new runs write `artifacts/simulation-results.json`. This is accelerated simulation and is not an FPS benchmark.
- `node scripts/browser-check.mjs`: actual keyboard and mouse driving, independent aiming, held fire, quick mine click, pause/resume, forced collapse for visual inspection, and screenshots. JavaScript errors are captured.
- `node scripts/play-matches.mjs`: three full rounds, one per vehicle, controlled through real Playwright key and pointer input. Read-only state and navigation assistance choose where to drive and aim. It never teleports the player or directly applies damage. Results record kills, deaths, weapon pickups and mine clicks.
- `node scripts/benchmark.mjs`: 90-second normal match, 60-second repeated stress, ten rendered resets with garbage collection, then at least 1,200 active simulation seconds of bot play with automatic completed-round restarts. The script writes incremental results outside Vite’s public directory, so output cannot reload the game. It rejects a lost counter or any unexpected page navigation.

## Timing interpretation

The first five seconds of each timing series are excluded. FPS is the reciprocal of mean observed requestAnimationFrame interval; p95 and p99 include browser scheduling. Simulation cost is CPU time across the fixed steps executed for a rendered frame. Rendering cost is CPU submission, not a separate GPU timer. Draw calls and triangles come from Three.js renderer statistics. These are live, on-machine observations, not guaranteed performance on other devices. The normal and stress samples are collected before the separate full-match input automation is launched. That additional Chrome window may run concurrently with part of the longevity session; longevity is a resource-stability check, and its frame timing is reported separately. Focus pauses do not count toward its 1,200 active seconds.

Stress injects 200 simultaneous swept projectiles and fills the shared physical fragment/wreck budget to 150, with 24 tank slots and repeated drum chains. It repeats these bursts every ten seconds. Counts naturally fall between bursts as shots collide and fragments expire; the diagnostic does not hold all three peaks continuously. `stressInitial` and peak counters preserve the observed loads.

The longevity script records body counts, geometries, textures, DOM nodes, listeners and retained JS heap after explicit collection. The recording buffer is bounded at 90,000 frame samples, so diagnostic memory rises until that buffer fills. Changes in live geometry/body counts also reflect currently active wrecks and debris; the ten equal round-reset snapshots provide the cleaner resource-leak comparison. The reported retained JS heap includes Rapier's WASM-backed state and diagnostic arrays; it is not total Chrome process or GPU memory.

## Findings and fixes made during validation

- Corner spawns originally left too much empty off-board space visible. Camera following now stays within central bounds while retaining a fixed overhead angle.
- User play feedback requested calmer, slower combat. Camera shake was removed entirely. Standard shell speed changed from 45 to 24 units/second; balanced tank speed from 9 to 6.5; standard firing interval from 0.65 to 0.85 seconds. Other vehicles and temporary weapons were slowed proportionally.
- Rapid right-click down/up could be lost between fixed steps. Mine clicks are now queued until a simulation command consumes them.
- Recursively detonating mines could invalidate a mutable index during mine iteration. The update now uses a stable identity list and checks membership before processing each mine.
- Stress deaths could push the shared debris/wreck list above its configured budget. Admission now evicts until the list fits, and a filled-budget simultaneous-death regression protects this case.
- A first seed sweep showed a side preference. Bot flank/strafe roles now use roster slot instead of global-ID parity; spawn slots, covers and weapon pickups have explicit 180-degree symmetry. An intermediate ten-seed sweep split wins five/five; the final hard-cap build sweep split four mint wins and six coral wins. Small seed sweeps are a balance smoke check, not a statistical fairness guarantee.
- Resource cleanup disposes temporary merged geometry and per-entity HUD materials. Label textures and fixed model components are shared and reused.

## Limits

These checks verify mechanics and exercise actual browser controls. They cannot certify subjective handling, competitive balance, or how readable every effect feels to a human. Tower collapse uses an authored support object and known side-rubble layout, with cosmetic rigid fragments. It is intentionally simpler than a general structural simulation. Bots can briefly hesitate in congestion and then reroute. The final performance notebook contains the measured results and any recorded runtime errors; no planned run should be interpreted as completed until its result appears there.

## Completed real-input rounds

The corrected input harness completed all three rounds without JavaScript errors. Scout: 50-48 team score, 5 player kills and 8 deaths. Balanced: 50-47, 6 kills and 6 deaths. Heavy: 47-50, 2 kills and 5 deaths. Input assistance chose routes and cursor targets, while all motion, shots, damage, pickups and respawns went through ordinary game controls and rules. See `artifacts/play-results.json` for the event evidence.

## Completed longevity session

The guarded soak completed 1,203.42 active simulation seconds in 1,215.27 wall seconds, with one initial page navigation and no reloads. Eight complete matches ran during the soak, in addition to the earlier stress match. The frame recording buffer reached its 90,000-sample cap. Textures stayed at 14; listeners varied with active menus between 39 and 42. The final explicit-GC checkpoint retained 52.74 MiB of JS heap, with 50 bodies and 11 physical fragments. Ten rendered round resets each restored 65 bodies and 355 rendered geometries. No JavaScript runtime errors were recorded.

## Final rendering optimization

After the completed 20-minute soak, physical debris rendering was changed from one mesh per fragment to a fixed 150-instance batch. Simulation, collision, fragment ownership, lifetimes and the hard cap were unchanged. A focused browser check rendered all 150 debris instances with 206 simultaneous projectiles and no runtime errors; ten subsequent rendered resets each returned 65 bodies, 351 geometries and 10 currently uploaded textures in that viewport. The final timing comparison uses the same seed as its preceding sample. The long-duration heap observations above belong to the pre-instancing renderer; post-change validation consists of the fixed-size buffer design, ten rendered resets, visual inspection and the timing/stress sample.

## Final 2560 x 1440 timing results

| Scenario                  | Average FPS | p95 frame | p99 frame | Mean simulation/frame | Mean render submission | Mean draw calls | Peak bodies / shots / fragments |
| ------------------------- | ----------: | --------: | --------: | --------------------: | ---------------------: | --------------: | ------------------------------- |
| Normal, 12 tanks          |       79.58 |   34.0 ms |   58.5 ms |               0.78 ms |                5.80 ms |             471 | 202 / 27 / 150                  |
| Repeated stress, 24 tanks |       69.66 |   34.5 ms |   73.2 ms |               1.17 ms |                8.57 ms |             973 | 213 / 200 / 150                 |

**The normal p95 <= 20 ms and p99 <= 33 ms targets were not met consistently.** Earlier short samples approached 120 FPS, but later checks showed substantial scheduling/frame-time variation. The same-seed debris-batching comparison reduced normal draw calls from 537 to 471 and stress calls from 1,143 to 973. Stress average FPS improved from 66.92 to 69.66, while normal average FPS varied from 83.19 to 79.58. This is a measured draw-call reduction, not a claim that every timing metric improved. Further frame-pacing investigation remains a known performance limitation.

Body totals include fixed ground/cover and dynamic tanks/wrecks/fragments; they are resident physics-body totals rather than an awake-island counter. Renderer confirmation: ANGLE Metal Renderer, Apple M3 Max. Chrome 152.0.7977.82. All reported final runs used a 2560 x 1440 render buffer. The notebook retains preceding timing samples and the full raw record.
