# Validation record

## Compact rank display and team score target — September 7, 2026

- Removed the XP bar, points and separate progression row. The current rank now sits beside the tank name; promotion mechanics and world chevrons remain. The updated veterancy browser check passed all rank transitions, absence of XP elements, and the 700 px layout; screenshots were refreshed.
- Team Battle now ends at 100 kills instead of 50. Menu/HUD labels use the shared score constant. The existing five-minute timer and Solo survival rules remain. Updated match regression checks that 50 and 99 kills keep playing and 100 wins.

## Solo survival — September 7, 2026

- Solo Assault now lasts until player death or a ten-minute survival finish. Unlimited replacements maintain at most six active enemies; neither the former 20-enemy limit nor the team 50-kill rule can end Solo. The existing scoreboard shows credited player kills and active enemies, and both end states preserve the final kill count. Restart resets it. Team rules remain five minutes/50 kills.
- **140 tests pass**. Updated Solo regressions exercise 120 kills without early finish, bounded tank/physics counts, fresh Rookie replacements, pause, death, ten-minute completion, reset and switching back to teams. Production build and whitespace checks pass.
- `node scripts/solo-survival-check.mjs` passed a rendered 120-replacement lifecycle test with seven tank models/health bars throughout, plus actual menu selection, ten-minute HUD, 55-kill live scoreboard, pause/resume, timeout score retention, death and restart. No JavaScript errors. Scoreboard screenshot inspected. Results: `artifacts/solo-survival-results.json`; screenshots: `artifacts/performance/solo-survival/`.

## Tank veterancy — September 7, 2026

- All tanks now gain per-life experience from actual enemy hull damage (1 XP/HP) and kills (+50 XP). Rookie → Veteran → Elite → Heroic thresholds are 0/200/500/1,000 XP. Bonuses increase damage, fire rate and max hull; Elite/Heroic repair slowly after five seconds without firing, laying mines or taking damage. Promotion preserves hull percentage and rescales pending reload/AI fire delays. Respawns reset progression before computing hull capacity. Old shells/mines and their explosive chains cannot award XP to a replacement tank.
- **140 tests pass**, including eleven new veterancy regressions covering shared damage credit, kill/overkill accounting, excluded damage, threshold boundaries/cap, multi-rank awards, all five weapon damage snapshots, both human/bot reload timers and rapid stacking, delayed ordnance and explosive chains, quiet-time repair/pause/death, all chassis/Solo scaling, repair pickups, respawn and snapshot isolation. Production build and whitespace checks pass.
- Ten full seeded matches and their body-count reset checks completed. There were **289 promotions**, **273 belonging to bots**, including **51 Elite** and **6 Heroic** promotions; the highest rank remains rare in this sample. `scripts/validate.ts` drains and counts promotion events each tick so the bounded event queue cannot discard earlier promotions. Results: `artifacts/simulation-results.json`.
- `node scripts/veterancy-check.mjs` passed real mouse aiming/firing → damage → XP → Veteran promotion, HUD progress, gold chevrons, toast and saved chime; bot promotion through actual damage; Elite repair delay and pause; Heroic max display; 700 px HUD fit; death/respawn reset. The production screenshots at 1600×900 and 700×700 were visually inspected. The 0.48-second promotion MP3 decoded and played to completion, and ten rendered resets held at 355 geometries. No JavaScript errors. Results: `artifacts/veterancy-results.json`; screenshots: `artifacts/performance/veterancy/`.
- A 25-second, 24-tank random-map browser run (first five seconds excluded from timings) produced six natural promotions at approximately **119.5 FPS**, **10.2 ms p99**, **0.334 ms mean simulation/frame** and **2.126 ms mean render submission/frame**. This is a smoke check, not a performance-improvement comparison. No new dependencies. The saved promotion chime adds 2,602 bytes; all eleven MP3s total 24,026 bytes.

## Rare laser point defense — September 7, 2026

- Added the requested automatic point-defense behavior: a small roof emitter fires a thin cyan/white beam at incoming enemy rounds, with **50% chance**, **six-second duration**, **seven-unit range** and one roll per shot/defending tank. A single central pickup appears after **25 seconds** and refills **45 seconds** after collection. The user's C&C Generals Zero Hour reference guides the automatic defensive feel; this is a temporary power-up with its own probability rules. Successful zaps vaporize rockets without causing a blast. Bots share the mechanic and leave a fresh laser pickup for another tank when already powered up.
- Integrated laser attempts into the earliest-contact projectile solver. Incoming relative motion, range entry and cover occlusion are checked before rolling; earlier world/hull hits remain authoritative. Failed attempts are remembered and consume solver budget once, so repeated frames do not turn the 50% chance into guaranteed defense. The timer pauses and clears on death/respawn/reset; normal ammo and firing cooldown remain independent.
- **129 tests pass**, including eleven new regressions for all five munition types, the probability boundary and 1,000 seeded samples, no rerolls, successful/failed damage outcomes, allies/outgoing/passing/out-of-range shots, dead/inactive defenders, fast swept entry, cover and hull precedence, moving defenders, 25 simultaneous failed defenses, duration/refresh/reset, rarity/refill and bot interest. The existing 100 random-map connectivity tests include the new center pickup.
- `node scripts/laser-defense-check.mjs`: actual W-key collection after the delayed spawn, pickup icon/toast, HUD countdown, a real-loop rocket interception with beam and saved zap, no splash damage, beam expiry, pause/resume and six-second effect expiry all passed. The production pickup and beam screenshots were inspected at 1600×900. The laser MP3 decoded and played to completion. Ten rendered resets retained 342 geometries in the inspected viewport. No JavaScript errors.
- Stronger-than-normal browser workload: all 24 tanks continually receive laser defense, full special ammo and rapid fire on a seeded random map. In 20 seconds (first five excluded from timings), **122 interceptions**, **119.48 FPS**, **10.0 ms p99**, **0.484 ms mean simulation/frame**, **2.219 ms mean render submission/frame**. This is a stress check, not a performance-improvement claim. Results: `artifacts/laser-defense-results.json`; screenshots: `artifacts/performance/laser-defense/`.
- Ten full seeded matches and ten body-count reset checks completed successfully (`artifacts/simulation-results.json`). Production build and whitespace checks pass. The new pickup PNG and saved laser MP3 regenerate byte-identically; all ten MP3s total 21,424 bytes. No dependencies added.

## Compact projectile models — September 7, 2026

- Replaced the shared stretched sphere with five distinct models: pointed standard shell, round spread pellets, finned rocket with a nose cone and short animated exhaust, rotating hexagonal ricochet puck, and cyan piercing dart. Every model retains a blue/red team marking. User size constraint is explicit: keep the projectiles compact. Rocket body length is **0.9425** world units versus **0.945** for the previous rocket, excluding its short exhaust. Other body lengths: standard 0.70, spread 0.324, ricochet 0.403, piercing 1.00. Maximum width is 0.52, across the rocket fins.
- Production geometry/materials are created once and instanced by ammunition type. Eleven layers cover all five types, with a combined 600-shot display cap, updated buffer ranges and no persistent trail objects. Visual updates leave shot state unchanged. Gameplay ballistics, damage and hitboxes are unchanged.
- All **118 tests** and the production build pass. `node scripts/projectile-visual-check.mjs` inspected the actual production models at gameplay/detail scales on both teams, checked compact bounds, rendered a 650-shot fixture capped at 600, verified aligned layers/finite transforms and clearing all instances, and performed ten rendered resets with a stable geometry count of 336 in that viewport. No browser errors.
- Chrome at 1600×900, 24 tanks on a seeded random map with continually refilled special ammo and rapid fire: **119.01 FPS**, **10.1 ms p99**, 0.399 ms mean simulation/frame and 2.364 ms mean render submission/frame; peak 35 live projectiles. Fifteen seconds total, first five excluded. This is a rendering smoke check, not a before/after speedup claim. Results: `artifacts/projectile-visual-results.json`. Inspected screenshots: `artifacts/performance/projectiles/`, including `comparison.png` and `live-combat.png`.

## Combat feedback and keyboard ammunition — September 7, 2026

- Completed all six small tweaks: credited-hit crosshair flash/tick, Q/E cycling and 1–5 direct selection, reload dimming, separate saved sounds for all five munitions, a gentle hull-panel pulse below 25% HP, and visible ammo supply pads with refill progress. Start/pause menus share the corrected controls. No dependencies added.
- All **118 tests** pass. New input checks cover one-shot keyboard commands, number-pad mappings, held-fire preservation, key-repeat rejection, inactive play, browser modifiers and editable controls. Damage regressions verify owner attribution on surviving/lethal hits and absence of damage events for protected, fully shielded and allied targets; each weapon's existing firing regression also checks its audio event metadata. Production build and whitespace checks pass.
- `node scripts/combat-feedback-check.mjs` passes in isolated Chrome at 1600×900: real Q/E and 1–5 events, wrap, empty-ammo rejection, preserved cooldown, ready/dim crosshair, all five actual firing sounds, hit attribution/exclusions, clustered-tick throttling and flash expiry. Low-health checks cover the 25% boundary, repair, pause/resume and death. Ammo pads stay visible at three refill stages, pause freezes progress, and crates return after 13 seconds. Production screenshots were inspected at 1600×900 and 600×780, including the narrow HUD and refill rings.
- All nine MP3 files decoded and played to completion in Chrome. Offline regeneration produced identical bytes for all nine files (20,200 bytes total), preserving the original four assets. Ten rendered round resets returned the same geometry count. No JavaScript errors.
- A 20-second, 24-tank random-map sample (first five seconds excluded) measured **119.40 FPS** with **9.7 ms p99** frame interval. This is a short smoke check, not a performance improvement claim. Full input/audio/reset evidence and timings: `artifacts/combat-feedback-results.json`. Screenshots: `artifacts/performance/combat-feedback/`.

## Bot steering and recovery — September 7, 2026

- Reproduced full-speed oscillation at a close goal (98 reversals in four seconds), alternating retreat/attack against a wall (35 reversals in ten seconds), and two allied hulls locked head-on. Replaced competing steering/separation vectors with actual-hull sweeps against cover and tanks, brief avoidance commitment, route lookahead and arrival braking. Recovery uses a separate destination and survives strategic/combat decisions. Added target/pickup/patrol continuity and moved blocked random-map goals to navigable neighbors. No damage, accuracy, fire-rate or role-stat increases.
- All **115 tests** and the production build pass. Seven new regressions exercise arrival at default/double speed across chassis, wall escape, both head-on destinations, committed recovery through repeated decisions/respawn, target continuity, pickup/patrol commitment and blocked route handling. Existing ammo, combat, solo modes, physics and 100 seeded map-connectivity regressions pass.
- `scripts/bot-movement-check.ts`: same three fixture setups and six 90-second runs (three seeds × village/random), before and after. A stalled window requests movement on >80 of 120 ticks but moves <1 m net; intentionally stationary firing does not count. Active windows differ because the bots' actions and match histories diverge. Saved reports: `artifacts/bot-movement-before.json`, `artifacts/bot-movement-after.json`.

| Measurement | Before | After |
| --- | ---: | ---: |
| Close-goal reversals, 4 seconds | 98 | 0 |
| Wall-retreat reversals, 10 seconds | 35 | 0 |
| Head-on allies, 10 seconds | Both stuck near the meeting point | Both finish within 0.25 m of their goals |
| Stalled / active movement windows | 211 / 2,787 (7.57%) | 19 / 2,679 (0.71%) |
| Stalled windows with ≥8 reversals | 9 | 0 |
| Total direction reversals | 4,518 | 1,227 |
| Mean simulation cost per tick, no rendering | 0.145 ms | 0.154 ms |

- The measured stall rate fell about **91%**. The extra hull queries add roughly 0.009 ms per simulation tick in this sample; the result is improved navigation rather than a CPU optimization. Some measured stalls remain, and six seeds do not prove every layout is free of congestion.
- Real Chrome at 1600×900: wall escape and head-on passing passed with zero reversals in each check. A 24-tank random-map run measured **119.47 FPS**, 0.387 ms mean simulation cost per rendered frame and 9.7 ms p99 frame interval (20 seconds total, first five excluded). No page errors. Results: `artifacts/bot-movement-browser.json`; production screenshots were inspected under `artifacts/performance/bot-movement/`.
- Ten full seeded matches completed and reset their body counts successfully; eight blue wins and two red, peak 99 bodies / 47 fragments. Output: `artifacts/simulation-results.json`. This sample is a regression smoke check, not a balance conclusion. Earlier ammunition work remains intact.

## Selectable ammunition and map crates — September 7, 2026

- Implemented finite, independent spread/rocket/ricochet/piercing inventory with unlimited standard, shared selection/refill/consumption rules, role-based bot preferences and no artillery rocket override. All tanks clear special reserves on death, respawn and reset. The eight mirrored ammo crates use four accessible route pairs; full tanks leave them for other recipients, with actual-receipt notifications and 13-second refill.
- `npm test`: **108 passing tests**. Coverage includes standard endurance, one unit per spread volley, all ballistics and rapid-fire intervals, no cooldown expenditure/bypass, selection before held fire, depletion fallback, inventory snapshots, death/reset clearing, caps/partial receipts, simultaneous recipients, refill timing, both cycling directions, input clearing, bot collection/depletion/role choice, village routes and **100 seeded random maps**. Piercing tests cover ordinary/rocket/piercing targets in both array orders, cross-tick repeated contact, spent allowance and kill ownership, tank stopping and thin-cover occlusion. Existing unrelated combat, physics and mode regressions pass.
- `npm run validate`: ten complete seeded matches and ten successful body-count resets; seven blue wins and three red, peak 115 bodies / 55 fragments. Completed in 26.8 seconds without rendering. Results: `artifacts/simulation-results.json`. This small seed sweep is a regression check, not proof of balance.
- `npm run build` and `git diff --check` pass. The existing large vendor-chunk advisory remains. No dependencies added. Four original pictograms are generated offline with `npm run generate:ammo` and saved in `public/textures/pickups/`; non-ammo art is unchanged.
- Real Chrome input passed: wheel event queued until the next simulation tick; forward/reverse/wrap; short trackpad-style bursts throttled; Shift-wheel zoom with both limits; selected ammo before held fire; final volley and standard continuation; right-click mine; pause-button clearing before a frame; ignored input while dead/paused; restart clearing. Production screenshots at 1600×900 and 600×780 confirm all five counts, selected/empty states and no HUD overflow. The final crate rim exposes the top-face symbol to the overhead camera. Screenshots: `artifacts/performance/ammunition/hud-crates.png` and `hud-narrow.png`. No page errors.
- Combat performance: Chrome 1600×900, 24 tanks, rapid fire, three seeds per scenario, five seconds warmup plus ten seconds measured per run. The abundant scenario continually refills spread/piercing to exercise more active shells; it is a stress comparison within this build, not a before/after optimization claim. Both scenarios use the same map seeds and roles. Results are in `artifacts/ammunition-results.json`; the cosmetic lid-rim adjustment was checked afterward with the input/visual suite.

| Scenario | Mean FPS | Simulation ms/frame | Render ms/frame | Mean run p99 frame interval | Peak shells |
| --- | ---: | ---: | ---: | ---: | ---: |
| Standard ammunition | 119.67 | 0.341 | 2.543 | 9.37 ms | 28 |
| Plentiful spread + piercing | 119.80 | 0.442 | 2.667 | 9.40 ms | 67 |

The entries below record earlier revisions; their timed/combined special-weapon checks have been replaced by the current ammunition regressions above.

## Code quality, performance and readability review — September 7, 2026

- Fixed mine commands lost between physics ticks, kept randomized towers on their authored axes, and shared tank creation/respawn physics. Reused navigation and camera buffers, cached 24 wreck assemblies, replaced cover scans with native collision filtering plus a handle map, selected AI/spawn candidates without sorting, compacted expired particles in place, and simplified effect presets and UI templates. Added typed tank parts/AI modes and unused-code checks. No new dependencies; production source grew about 0.6% and the game JavaScript chunk grew 0.48 kB (0.32 kB gzip).
- All 82 tests and the strict production build pass. Regression coverage includes reused navigation after blocked goals/topology changes, cover-handle removal/reset, all 24 cached wreck bounds and independent transforms, 100 random maps, and respawn friction/restitution. An intentionally invalid profiler weapon fails TypeScript; browser errors abort profiling and incomplete runs cannot replace notebook data.
- Chrome checks passed for a right-click between physics ticks and exactly-once consumption during catch-up, movement/fire, pause/resume, zoom and destruction. Ten rendered death/reset cycles retained exactly 436 geometries and 19 textures each, with no page errors. Local output: `artifacts/performance/browser-controls.json`. This is a reset check, not a long-session soak.
- Ten full seeded simulations completed with seven blue wins and three red; all physics reset counts matched. Peak 107 bodies and 50 fragments. Saved to `artifacts/simulation-results.json`.
- Initial 1440p Chrome comparison: three seeded 20-second runs per scenario/build, excluding five seconds, plus separate CPU profiles. Normal render CPU 1.876→1.477 ms/frame; stress 2.459→2.589 ms/frame. A same-seed follow-up alternated original, updated, updated, original builds using the same profiler workload. Normal render CPU 1.571→1.495 ms (−4.8%); stress 2.633→2.761 ms (+4.9%). Simulation CPU fell 3.9% normal and 5.2% stress in that repeat. FPS stayed around 118–120, normal p99 stayed 9.35 ms, and stress p99 rose 9.30→9.45 ms. Draw calls were effectively unchanged. Timing varies and the stress regression remains in these samples; no overall speedup or smoother pacing is claimed. Both comparisons are retained in `artifacts/performance-results.json` and shown in the notebook.

## Chrome rendering profile and optimization — September 6, 2026

- Captured four real 20-second Chrome CPU profiles plus six 40-second live gameplay measurements at 2560×1440, seed 207 and 115% speed. Rendering dominated sampled CPU work; simulation averaged 0.20–0.33 ms/frame.
- Reduced arena tree geometry 34,368→8,736 triangles, boundary trees 68,736→3,744, fences 28,080→3,120. Batched 24 boundary trees into two rows. Upload only changed track spans and active effect instances. House/pickup textures, collisions, effects and track fading remain intact.
- Conservative final crowded repeat: draw calls 443→400, rendered triangles 361,811→296,232, render CPU 1.546→1.485 ms, GPU median 2.129→2.046 ms. FPS remained about 120; p95 frame interval remained 9.3 ms. Larger GPU improvements in the initial run were variable and are not the claimed stable result. Sustained 111 FPS was not reproduced.
- All 64 tests and production build pass, including new scenery budgets and track ring-wrap upload coverage. Compared model appearance through the production renderer in Chrome. Details and raw Chrome profiles: [profiling report](artifacts/profiles/README.md).

## House optimization and health-bar behavior — September 6, 2026

- Removed rounded bevel geometry from house trim while retaining all textures and architectural detail. Across 14 houses: original pre-detail geometry 67,720 triangles; detailed version 112,128; optimized detail 17,088. Textured houses still batch into three meshes each (42 total).
- Same static production scene at 1920×1080, 30 warmup plus 240 measured frames per mode: before 487,808 rendered triangles / 401 calls / 122.0 FPS / 1.27 ms render CPU; after 351,488 triangles / 401 calls / 122.2 FPS / 1.22 ms CPU. FPS was effectively unchanged in this refresh-limited sample; the geometry reduction is confirmed. Raw results: `artifacts/house-optimization.json`. The user's active game continued separately.
- Added a 20,000-triangle budget regression for arena houses. All 61 preceding tests and the new budget check pass; production build passes. Inspected the optimized scene with no browser warnings/errors. Temporary benchmark source/tab were removed.
- Matched V-Tanks' health-bar behavior: fixed left edge, outlined dark track, yellow at <=60% and red at <=30%; player HUD shares these thresholds. Rendered checks confirmed 0.6 and 0.3 fill scales with an unchanged left anchor. Shield state remains in the effects text.
- Removed the tower notification label and the UI branch that displayed it. Tower destruction still clears its passage.

## House surface details — September 6, 2026

- Added shared procedural shingle and wood-siding textures with subtle bump detail, plus shutters, window frames/sills, door panels/steps, corner boards, eave trim, ridge caps and chimney brick courses. Collision shapes are unchanged.
- Production build and all 61 existing tests pass. Inspected both roof colors and multiple sides in a rendered, batched model preview. No JavaScript errors; the temporary preview emitted a shadow-map deprecation warning from its own lighting setup and was removed after inspection.

## Playtest controls, visual polish and shootable mines — September 6, 2026

- Applied a shared 13% increase to tank and projectile base speeds. Temporary pause sliders scale each category independently from 50–200%, persist locally, update existing shells and collision prediction, and retain class/weapon ratios. Verified both controls render with the user's saved 115% settings; those preferences were preserved.
- Shells and spread pellets now start at the model-derived muzzle, including render height. A swept obstruction check prevents barrels from spawning shots through nearby cover or enemy hulls.
- Expanded names to 84 and shuffle an independent seeded deck per round; names remain stable through respawn.
- Pickup cubes grew from 0.8 to 1.25 m, with original cached pictogram textures on every face. Collection produces 24 sparks, an expanding ring and a brief tank glow. Pine trees gained staggered foliage, branch clusters, bark and roots; destruction emits 96 green/brown particles and nine bounded physical fragments.
- Direct swept shell hits detonate friendly/enemy mines, armed or arming. Contact ordering respects cover; removal occurs before blast chains and kill credit follows the shooter.
- All 61 tests, strict TypeScript, production build and whitespace checks pass. Ten complete simulated matches and ten reset checks pass; seven blue wins and three red. Current results are in `artifacts/simulation-results.json`. This is not a rendering benchmark.
- Browser inspection confirmed the pickup symbols, detailed trees, collection burst and tree debris using the production renderer in a temporary controlled scene. No browser warnings/errors were reported; the preview was removed and its tab closed. The user's active game was preserved.

## Tank-to-tank contact — September 6, 2026

- Added a massless contact collider measured from the same hull/track geometry as combat. It collides only with other tank hulls; cover and ground use the existing compact collider. Both spawn and respawn create it, and body removal cleans it up.
- Added predictive contacts based on boosted travel per tick. Without them, the new regression reproduced a brief 0.35 m overlap at boosted impact despite the larger shape. With them, 24 combinations of chassis, front/side impacts, orientation and friendly/enemy contact stay within 1 cm of the model-derived boundary under sustained drive impulses.
- All 54 tests and the production build pass. Ten complete simulated matches split five wins per team; ten body-count resets pass. Updated simulation results are in `artifacts/simulation-results.json`. No rendering benchmark was rerun.

## Model-derived hitboxes — September 6, 2026

- Removed the copied hull dimensions. Combat boxes are now measured directly from `tankModel` hull/track geometry and its transforms, once per chassis. Model size and proportions automatically carry into hit detection; only the intentional 0.18 m shell allowance remains separate.
- Boundary regression checks compare hits and misses 1 mm inside/outside every measured hull edge for all three chassis.

## Hull hit registration — September 6, 2026

- Reproduced missed outer-track, nose and grazing hits with three failing tests against the previous compact movement collider. Combat now uses the rendered hull footprint plus a 0.18 m shell-radius allowance, including the longer heavy chassis and tank translation during each simulation tick.
- 52 tests pass. Coverage includes all chassis, rotated hulls, visible mesh bounds, clean misses, moving-target crossings, cover, friendly tanks, shields and spawn protection. Production build and whitespace checks pass.
- Ten complete simulated matches and ten body-count resets pass. Blue won seven and red three, including one overtime; this is a smoke check, not a balance or rendering benchmark. Updated results are in `artifacts/simulation-results.json`.

## Bot personalities, track fading and base speed — September 6, 2026

- Adapted seven local V-Tanks roles plus a hunter variant on every tenth bot slot. Source behavior lives in `src/game/bot-personalities.ts`; the controller retains pathfinding, team-neutral targeting, reaction delays and imperfect aim. Support escorts; artillery uses existing rockets. Added personality labels were removed at the user's request.
- Base speeds increased 20% to 9.826 / 7.924 / 6.022 m/s (displayed as 35 / 29 / 22 km/h). The speed pickup remains +50% for 12 seconds.
- Tracks fade from age 4 to 18 seconds. A full ring buffer waits for a slot to fade completely before reusing it. A controlled rendered preview showed progressively lighter trails at 2, 8, 13 and 17 seconds, and invisible trails at 19 seconds. The temporary preview was removed after inspection.
- 45 tests pass, including role assignment, rare-hunter distribution, stand-off movement, stationary firing lanes, mine use, blocked line of sight, artillery pickups, the player's cadence edge and track-buffer overwrite protection. Production build passes. Ten complete simulated rounds and ten body-count resets pass; blue won four and red six, with one overtime. Current output is `artifacts/simulation-results.json`.
- Live autoplay was inspected with no reported browser errors; final tanks retain team markers and health/reload bars without personality labels. This pass did not rerun the long rendering benchmark.

## V-Tanks mechanics and tread trails — September 5, 2026

- Verified the reference checkout matches GitHub main at `570bf8dd46a48c0761a4faccafa40197a821267a`. Inspected the deployed game's mission/controls UI and made brief keyboard/fire attempts; exact powerup values and interception rules are source-verified, not claimed as independently collected during play.
- `npm test`: 39 passing tests. Added continuous interception, asynchronous crossing misses, earliest-contact ordering, thin-wall occlusion, ricochet interception, tank-hit/expiry precedence, opposing kill credit, stacking/expiry, finite shields, speed/diagonal/braking checks, and distance-spaced/capped/resetting tracks.
- `npm run validate`: ten complete five-minute simulated matches and ten successful body-count resets. Blue won six and red four; all towers were destroyed. These are accelerated simulation checks, not a rendered performance benchmark. The current run is in `artifacts/simulation-results.json`.
- `npm run build`: strict TypeScript and production bundle pass. The existing large-chunk advisory remains.
- In-app browser: verified the 29/24/18 km/h selector, gameplay rendering, twin tread trails on the village roads, and pause/resume. No browser warnings/errors were reported in the inspected live session. Brief key presses are limited for subjective handling assessment; user play remains the acceptance test for feel.
- Synthetic 200-shell simultaneous burst, projectile step only in Node: ten measured samples after two warmups averaged 10.03 ms, maximum 11.13 ms. This intentionally dense case includes repeated collision resolution; it is not browser FPS. Track rendering adds one instanced draw call with at most 8,192 treads; fading runs in the shader. The long browser benchmark was not rerun.

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
