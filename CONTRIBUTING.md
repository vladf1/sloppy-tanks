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

Cached geometry and materials outlive round resets. Only per-instance resources marked `userData.owned` are disposed by round cleanup. `isMesh` retains concrete Three.js field types after an `instanceof` check. Keep bounded capacities for particles, physics fragments, track marks and diagnostics.
