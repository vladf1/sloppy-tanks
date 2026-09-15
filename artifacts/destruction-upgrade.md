# Physics and destruction upgrade

## Behavior

- Explosions, mine/drum/rocket chains, shell interceptions and tank deaths kick nearby physical debris through the existing Rapier world. Living-tank blast damage and impulses are unchanged.
- A squared distance falloff and vertical distance check limit the blast; upward impulses at off-centre points produce real tumbling. Sleeping bodies wake, with impulse leverage scaled down for small chips.
- Wrecks initially survive their flight plus 3.2 seconds. A later blast can give substantial pieces up to five more seconds to finish flying, subject to an absolute 18-second deadline and the shared 80-fragment FIFO budget. Pieces already in their final half-second fade are not relaunched.
- Cargo releases three wooden panels and a frame beam; timber releases three beams; trees release a substantial bark/end-grain trunk and a branch; drums release a shell and lid; towers release two deck sections and two posts. Existing tree particles, chips, foliage and tower foundations remain.
- New components use simple cuboid/cylinder colliders, per-material friction/restitution, sleeping, and no additional CCD. Existing wreck CCD is retained. Instanced component rendering shares geometry and materials.
- Authored pieces use a separate seeded motion stream. Legacy destruction RNG advancement is retained so changing the authored component count does not reshuffle combat random draws. Replays remain deterministic within this build; whole-match trajectories are not promised bit-identical to an older build.

## Dragon's teeth

The original tapered convex collider is now a dynamic body of mass 6.9984 (10% smaller in each dimension and 27.1% lighter than the previous 9.6; tank masses are around 1–2 in these game units), friction 1.15 and restitution 0.02. The existing tank-only contact footprint remains attached with zero added mass; it still does not block projectile queries. A dedicated cover filter adds ground contact while retaining existing cover membership and debris/tank collision roles.

Projectile hits apply an impulse at the actual planar hit location, before any ricochet. Standard shells shove concrete; piercing and rocket impulses are stronger. Explosions use the shared debris blast path with force divided by the concrete body's mass. Repeated hits move the same object; a strong nearby explosion can flip it completely. No health, scoring, weapon damage, match rules or AI difficulty values changed.

Visuals follow the body's full transform. AI reads a conservative rotated footprint; navigation patches cover both the old and new locations. Patches are checked at most four times per second per block, after a 0.5-metre change or a final settling correction. Existing local shape-query avoidance handles movement between patches.

Hedgehogs now share the movable-cover blast and navigation path. Each uses one dynamic steel body of mass 6 with nine convex web/flange colliders, retaining the visible openings. Every collider maps to the same cover entity for projectile handling. Their Three.js scale is accounted for when centering the model on its rigid body.

## Contact effects seam

Substantial bodies emit bounded `debris-impact` simulation events containing material, identity, position, height and Rapier contact-force magnitude. Contact thresholds scale with body mass; events are limited to eight per tick and one per body per 0.3 seconds. These are available for future wood/concrete/metal impact sounds and particles. Presentation currently leaves this telemetry silent.

## Verification

- `npm run check`: lint, formatting, typecheck/build and 190 tests passed.
- `npm test`: 190/190 passed.
- `npm run validate`: ten complete seeded matches, reset body counts checked; 30.44 seconds versus a fresh 30.50-second baseline. Peak 113 bodies and 50 fragments versus 137/76 in the baseline match sample.
- Ten new regression tests cover blast falloff and vertical range, sleeping-body wakeup, actual turret/drum secondary launches, real projectile contact and stronger blasts, cumulative concrete impulses, navigation displacement and settling, object-specific pieces/materials/contacts, budget/lifecycle/reset/collision filtering, replay determinism, legacy RNG preservation and an actual bot traversing a route around moved concrete.
- Existing cargo, quarry-contact and wreck-lifetime expectations were updated to match the requested behavior.
- Browser reset fixture passed 15 map switches with stable GPU resources (519 geometries / 45 textures after warmup).
- Browser inspection confirmed recognizable airborne wood/trunk/drum/tower pieces, a moving/tipping tooth, and a landed turret relaunched and rotated by a newly destroyed drum.

## Performance

These measurements cover the initial upgrade, before the later tooth resizing and movable-hedgehog tuning.

Final browser measurements are recorded in `artifacts/performance-results.json`; full captures and profiles are in ignored `artifacts/performance/{before,after}`. Both sides use the same Chrome version, 1280×720 viewport, seeds 12345/45678/98765, five-second warmup and fifteen-second samples. Normal gameplay and the existing 24-tank/max-fragment/multiple-drum stress workload were measured.


Three-seed averages from the final matched runs:

| Metric | Before | After |
| --- | ---: | ---: |
| Normal FPS | 119.0 | 119.6 |
| Normal simulation ms/frame | 0.226 | 0.250 |
| Normal render ms/frame | 3.91 | 4.27 |
| Normal draw calls | 876 | 896 |
| Stress FPS | 112.0 | 110.2 |
| Stress simulation ms/frame | 0.522 | 0.544 |
| Stress render ms/frame | 8.09 | 8.26 |
| Stress draw calls | 2292 | 2347 |
| Stress maximum bodies / fragments | 175 / 80 | 175 / 80 |

Normal FPS stayed effectively unchanged. Stress FPS decreased about 1.6%, with about 2.4% more draw calls. Stress frame-time p95 ranged from 9.3–16.7 ms after, versus 9.3–9.4 ms before: average throughput remained close, but two seeds had worse frame-time tails. These are short local measurements, not a claim of zero performance cost. No browser errors were recorded.

The benchmark no longer forces an oversized 2560×1440 viewport. Its default 1280×720 viewport and canvas fit the window; explicit `SLOPPY_WIDTH` / `SLOPPY_HEIGHT` overrides remain available.

A separate reproducible physics workload starts with 80 fragments, twelve tank wrecks, ten destroyed crates and sixteen concrete blocks, then triggers three simultaneous blasts each simulated second. Run `node --import tsx scripts/destruction-benchmark.ts`. Results are saved to `artifacts/destruction-benchmark.json`.

The isolated physics benchmark averaged 0.032 ms per Rapier step before and 0.064 ms after, excluding each side's first JIT warmup run. Both peaked at 97 bodies. At ten seconds, 17 of 20 remaining dynamic bodies were sleeping in the upgraded workload; transient bodies continued through the bounded cleanup lifecycle.

## Files

- Physics: `src/game/debris-physics.ts`, `scenery-pieces.ts`, `movable-cover.ts`, `simulation.ts`, `damage.ts`, `projectiles.ts`, `fragments.ts`, `wrecks.ts`, `types.ts`, `data.ts`.
- Rendering and collider documentation: `src/game/presentation.ts`, `tree-models.ts`, `quarry-barrier-shapes.ts`.
- Tests and visual fixture: `tests/destruction-physics.test.ts`, `destruction.browser.html`, `game.test.ts`, `harbor.test.ts`, `quarry.test.ts`.
- Performance tooling: `scripts/destruction-benchmark.ts`, `scripts/profile.mjs` and the result artifacts listed above.
- Check configuration: `.prettierignore`, `tools/lint/eslint.config.mjs` now exclude the existing generated `dist-cloudflare` directory, which previously caused `npm run check` to lint bundled vendor code.

## Deliberate limits and useful follow-ups

Scenery pieces are authored approximations, not arbitrary mesh fracture. Debris does not damage living tanks or collide with other debris. Navigation is conservative and throttled; it is not a per-frame remeshing system. Old pieces still expire or are evicted under load. The next useful polish is material-specific contact audio/dust using the existing event hook, or a few more authored roof/branch variants after visual review.

The interactive fixture is at `/sloppy-tanks/tests/destruction.browser.html` while Vite is running. It provides repeatable scenery breakage, concrete hits/blasts, and the landed-wreck/drum sequence.
