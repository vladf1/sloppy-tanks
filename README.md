# Sloppy Tanks

An original, local 3D toy demolition game: one human and eleven bots, blue versus red, a destructible yard, and quick respawns. Built with plain TypeScript, Three.js, Rapier and Howler. No React or UI framework.

## Run

```sh
npm ci
npm run dev
```

Open the Vite URL printed in the terminal (normally http://127.0.0.1:5173/sloppy-tanks/). The performance notebook is at `/sloppy-tanks/benchmark.html`.

```sh
npm test           # focused simulation regression tests
npm run validate   # ten complete seeded matches and reset checks
npm run build      # strict TypeScript and production bundle
node scripts/browser-check.mjs  # actual Chrome keyboard/mouse smoke test
node scripts/benchmark.mjs      # 1440p Chrome normal, stress, ten resets, 20-minute session
```

The browser scripts use installed Google Chrome and isolated temporary profiles. They do not access the user's existing Chrome profile. The benchmark takes approximately 23 active minutes; sleep or focus pauses extend it. It writes `artifacts/benchmark-results.json`; the notebook reads that file. `artifacts/simulation-results.json` contains accelerated, non-rendered match validation. These are deliberately labeled separately.

For a shorter before/after comparison, run `node scripts/profile.mjs before` on the original revision and `node scripts/profile.mjs after` on the changed revision with Vite running. Each pass records three seeded 20-second normal runs and three stress runs at 1440p, excluding the first five seconds, plus separate Chrome DevTools CPU profiles. Avoid source edits or other browser workloads during measurement. Full results and importable `.cpuprofile` files stay locally in `artifacts/performance/`; the compact comparison is saved to `artifacts/performance-results.json` and shown at the top of the performance notebook.

The September 5 rendering pass combines compatible model colors into vertex-colored batches and freezes stationary scenery transforms. Three-run mean render CPU time fell from 1.59 to 1.19 ms/frame in normal play and 2.74 to 2.00 ms/frame under stress; draw calls fell from 655 to 329 and 1,176 to 668. FPS stayed near 120. Stress p99 increased from 9.3 to 10.9 ms, so these results demonstrate lower submission cost, not improved frame pacing. Geometry, shadow settings, resolution and gameplay rules are preserved. The 30 tests and production build pass; these short measurements do not replace the earlier longevity test.

## Static textures

`npm run generate:textures` runs `scripts/generate-textures.ts` using the development-only Node canvas library. It writes seven power-up faces to `public/textures/pickups/`, siding and shingles to `public/textures/houses/`, two team symbols to `public/textures/teams/`, and grayscale armor wear to `public/textures/tanks/armor-wear.png`. The shared 512 × 512 armor map adds visible paint wear, panel seams, scratches and bump detail while retaining each tank’s team color. These PNGs are checked in; local dev/build commands use them directly. Local development and GitHub Pages builds load/copy the saved assets directly; they do not run the generator. Run the offline command and commit the updated images only when changing the artwork or pickup colors. Font rendering can vary across operating systems.

AI-generated grass and dirt are separate: runtime WebPs are in `public/textures/ground/`, with original PNGs and prompts in `assets/texture-sources/`. The offline script does not overwrite those images.

## Play

- **WASD**: screen-relative movement.
- **Mouse**: independent turret aim.
- **Hold left mouse**: fire.
- **Right click**: drop a mine; 0.8-second arming delay and 7-second cooldown.
- **Wheel**: zoom.
- **Escape** or **Pause**: pause; resume from the menu.
- Losing focus clears controls and pauses. Hidden tabs stop simulation and rendering.

Choose Skipper (80 HP), Bruiser (100 HP), or Big Rig (140 HP). Standard shells deal 40 damage. The human's team is chosen randomly. Blue uses diamonds; red uses twin bars. Health and reload bars float above vehicles, and the human has a yellow ground ring and a brief spawn pulse.

Rounds last up to five minutes or end at 50 team kills. A timed tie enters next-kill overtime. Death launches a cosmetic physical wreck with a random tumble axis per part. One in four breakups sends the turret 20–30 units higher, while ordinary arcs stay at 4.5–8 units. Pieces persist through their flight, then clear. Death gives a three-second respawn window with vehicle selection. A respawn has two seconds of protection, cancelled by firing. Spawn selection considers enemy distance, line of sight and friendly congestion.

Pickups are collected by driving through them. Spread and breaching rockets each last 14 seconds and combine: collecting both fires three explosive rockets per volley. Their timers expire independently; collecting the same pickup refreshes its timer. Four repair pickups sit on the west, east, north and south routes. Rapid fire halves reload time; ricochet doubles damage and adds two bounces. These upgrades last 12 seconds and combine with each other and special weapons. Speed adds 50% for 12 seconds. Shield absorbs 120 damage (three standard shells) or expires after 15 seconds; excess damage reaches the hull. Repair fully heals. Repeated upgrades refresh their timer or shield capacity without multiplying their strength. Mines are independent of weapon pickups. Allies do not take damage or block projectiles. Self-inflicted explosions can kill the owner without awarding a point. Drum and mine chains preserve the initiating damage owner.

The current pacing keeps standard shell speed at 21.696 world units/second and bot weapon interval at 0.85 seconds. The human fires 20% faster (about 0.71 seconds per standard shot); the same advantage applies to special weapons and rapid-fire upgrades. Bots also retain their extra aiming/fire delays. Balanced movement is about 8.95 units/second, adapted from V-Tanks as described below. **There is no camera shake.** Impact feedback comes from recoil, sparks, fragments, light and sound. Surviving a hull hit gives the tank a brief 0.28-second visual jolt, a gold/white spark burst and clear health loss in the overhead bar. This does not move the physics body or shake the camera; fully shield-absorbed hits do not trigger hull-damage feedback.

## Destruction and navigation

Timber panels and walls break independently. Drums explode. Each tower has one authored support-health object; removing it creates a burst of prebuilt fragments and two persistent side-rubble colliders, leaving its middle traversable. Foundations and permanent barriers survive the round. Flying debris and wrecks collide with ground and cover, but cannot damage or trap living tanks.

The 48 × 48 navigation grid has conservative clearance for vehicles and cardinal A* paths. Destruction rebuilds the affected cells immediately. Bots replan on topology changes and use Rapier queries for visibility and nearby vehicles. Their staggered decisions choose combat, pickups, retreat or flanks; reaction time, aim error, brief memory and congestion recovery constrain them. There is no special human-target priority.

## Code map

| File                                          | Responsibility                                                  |
| --------------------------------------------- | --------------------------------------------------------------- |
| `src/game/types.ts`                           | Typed commands, entities and simulation events                  |
| `src/game/data.ts`                            | Vehicle, weapon, pickup definitions and seeded RNG              |
| `src/game/arena.ts`                           | Authored cover, pickups, spawn positions                        |
| `src/game/simulation.ts`                      | Fixed-step world, movement, respawns, reset, snapshot boundary  |
| `src/game/weapons.ts`                         | Swept shells, ricochets, mines, pickup effects                  |
| `src/game/damage.ts`                          | Damage ownership, kill credit, chain reactions, destruction     |
| `src/game/navigation.ts` / `ai.ts`            | Clearance grid, A*, bot perception and commands                 |
| `src/game/models.ts` / `presentation.ts`      | Original geometry, batching, interpolated scene, effects        |
| `src/game/controls.ts` / `audio.ts` / `ui.ts` | Input, spatial sound, minimal menus and HUD                     |
| `src/main.ts`                                 | Bounded fixed-step loop, system wiring, development diagnostics |

Physics runs at 60 Hz with at most five catch-up steps. Dynamic live tanks lock roll and pitch, using acceleration-limited impulses that preserve external knockback. Death unlocks rotation and retains momentum. Projectiles use swept Rapier queries. Tank hits use a hull-sized combat box with 0.18 m shell allowance and account for tank translation during the physics tick; model-derived tank-to-tank contact boxes use the unpadded hull footprint, while compact colliders remain for cover and ground. Predictive contacts account for boosted closing speeds to prevent initial impact overlap. Cover still uses its physical collider. Combat dimensions are calculated directly from each rendered hull and its tracks, including model scaling, and cached once per chassis. Regression tests check hits and misses immediately around all four model-derived boundaries. Human and bot controllers produce the same `VehicleCommand`. Stable IDs, a seed and a read-only snapshot provide a future command/snapshot boundary; networking and replay synchronization are not implemented.

Static authored mesh parts are batched by material. Projectiles, physical debris visuals, and short-lived visual particles are instanced. Physical fragments are capped at 80; visual particles at 1,200. Temporary merged geometries and per-entity resources are disposed when removed or reset. One directional sun casts shadows; the brief explosion light does not.

## Development diagnostics

`?autoplay` uses a bot command for the human slot. `?tweak` loads development-only Tweakpane for zoom. In development, `window.sloppy` exposes `sim.snapshot()`, `record()`, `stop()`, `stress()`, `collapse()`, `overview()` and `exactResolution()`. Production omits these controls and Tweakpane.

## Initial boundaries and remaining tuning

One arena, desktop keyboard/mouse, local settings, no networking, no currency or progression. All models are original generated geometry; no Roblox assets were extracted. Sounds are synthesized locally. The optional display font loads from Google Fonts, with system fallbacks.

Tower collapse is authored destruction with temporary rigid fragments, not a structural engineering simulation. Bots can briefly hesitate or push in crowded choke points, then replan. Much of the weak cover can be destroyed early; permanent late-round cover remains. Vehicle and pickup balance, AI accuracy, visual threat readability, and the rate of destruction remain subjective tuning areas. The WASM-containing production bundle is approximately 1.26 MB gzipped, and Vite reports a chunk-size advisory.

Before the arena and visual redesign, 1440p timing averaged 79.6 FPS normal and 69.7 FPS stress; p95/p99 frame-time targets remain unmet. The 23 tests, three real-input rounds, and 20-minute simulation soak completed. See the performance notebook and VALIDATION.md for sample conditions, the post-soak rendering optimization, and remaining limits. Browser automation verifies inputs and full match flow; it does not substitute for human feel testing.

## Readability and arena update

The yard is now 120 × 120 units (formerly 72 × 72): 2.78 times the area, with wider flanks and repositioned cover, pickups, spawns and navigation. Warm sand, blue service lanes, white concrete and saturated blue/red teams replace the olive palette. Shells have larger colored bodies, white cores and dark contrast rims; their slower speeds are retained. Camera shake remains absent. Tank models use sloped armor, rounded or faceted turrets, longer barrels, exposed road wheels and curved track belts. Each tank explosion leaves three recognizable parts rather than eight cubes, plus a brief wreck; all clear within 2.6 seconds. The start button reads “LET’S GO.”

Current validation is recorded at the top of VALIDATION.md. Older timing, full-input rounds and longevity results describe the preceding build.


Latest tuning: the camera tracks the player's interpolated position at screen center, including at arena edges. Tank proportions are longer and lower, with faceted armor, rear turret bustles, hatches, exhausts and visible wheel hubs. Shells were reduced and then enlarged 50% from that smaller size following play feedback. Bots use less precise leading, wider aim error, slower turret tracking and an extra pause between shots; human weapon cadence is unchanged.


## Pine Village

The latest reference pass replaces the service yard with a 120 × 120 village: 14 pitched-roof cabins, 12 collidable pines, breakable wooden garden fences, two watchtowers, dirt lanes, textured grass and team spawn flags. Twelve cabins remain as permanent cover so the village retains its routes throughout a round; the two central cabins, trees, fences and towers can be demolished. The outside tree line is scenery beyond the arena boundary.

Tank silhouettes now follow the supplied examples: tall cast or angular turrets, thick gray guns, broad dark tracks and prominent gray armor panels. Every projectile uses its owner's blue/red team color with a white core. The outlined aiming reticle stays visible over terrain and cover. All shell speeds are 20% lower (standard 19.2, rapid 21.6, spread 17.6, rocket 13.6, ricochet 22.4 units/second); lifetime increases to 3.5 seconds to preserve travel range. Shell size, player-centered camera and reduced bot accuracy/cadence are retained.


## Movement, breakup and quick selection

Clicking a vehicle card starts immediately. The selector displays rounded road speeds: Skipper 35 km/h, Bruiser 29 km/h, Big Rig 22 km/h. Simulation uses the precise speeds below.

Destroyed tanks separate into actual hull and turret models. In 40% of breakups the barrel detaches too; otherwise it stays on the spinning turret. Planned landing separation is roughly 14–28 world metres, constrained by arena and visible-view margins. One in four breakups launches the turret 20–30 metres above its starting height; ordinary arcs rise 4.5–8 metres. Each piece gets an independent, uniformly random tumble axis and spin speed. Physical collisions can shorten or redirect a throw. High launches may outlast the three-second respawn; pieces clear after their planned flight plus 1.8 seconds, within the shared 80-piece cap. The compact respawn strip leaves the effect visible. Pieces remain cosmetic and cannot damage or obstruct living tanks.

## Publishing

Play at https://fridman.me/sloppy-tanks/. Every push to `main` runs the tests and production build in GitHub Actions, then deploys `dist/` to GitHub Pages after they succeed. You can also run the workflow manually from the Actions tab.

The Vite base path is `/sloppy-tanks/`. GitHub Pages inherits `fridman.me` from the account site.


## V-Tanks movement, upgrades and tracks

Reference: [V-Tanks](https://fridman.me/v-tanks/), verified against `vladf1/v-tanks` revision `570bf8dd46a48c0761a4faccafa40197a821267a`. Its balanced tank travels at 184 source units/second and its standard shell at 535. Scaling that ratio to our 19.2 m/s shell and adding the requested 20% base-speed increase gives 7.924 m/s; light and heavy use its 1.24 and 0.76 class multipliers (9.826 and 6.022 m/s). A further shared 13% speed increase brings light/balanced/heavy to 11.103 / 8.954 / 6.805 m/s and standard/spread/rocket projectiles to 21.696 / 19.888 / 15.368 m/s. Acceleration/braking is 100 m/s² and hull rotation is capped at 9 radians/second. This adapts the dodge timing and responsive handling to our 3D arena; screen-space speed still depends on zoom.

Opposing shells intercept continuously, including between simulation ticks and after ricochets. Allied shells pass through each other. The earliest wall, tank, expiry or shell contact wins; thin cover blocks interception. Both shells disappear with a small blast that deals one standard 40-damage hit to nearby tanks on either team, credited to the opposing shell's shooter. Ordinary blast radius is 3 m; intercepted rockets use 5.3 m. This blast does not damage cover or trigger mines.

All moving tanks leave paired tread impressions following their hull heading. Marks are distance-spaced, fade progressively from 4 to 18 seconds, and use one instanced draw call capped at 8,192 treads. A full buffer skips new impressions until the oldest pair has completely faded; visible marks are never overwritten abruptly. Tracks freeze during pause, clear with a new round, and are cosmetic. Camera shake remains absent.


## Bot personalities

Adapted from the local V-Tanks enemy profiles:

- **Scout:** fast approach, close fighting range and loose aim.
- **Guard:** holds medium range, retreats when crowded and strafes across firing lanes.
- **Sniper:** moves into a long firing lane, stops to aim, and retreats from close threats.
- **Heavy:** slow advance with a deliberate firing rhythm.
- **Minelayer:** closes in and deliberately drops mines near opponents.
- **Support:** escorts nearby teammates and fights from farther back.
- **Artillery:** takes a distant position and fires slow breaching rockets; special pickups temporarily override its normal rockets.

Every tenth bot slot is an aggressive **Hunter** variant (one of eleven bots in a normal round). Hunters pursue through cover using navigation, close to short range and turn faster, but still need line of sight to fire. Personality reloads retain a floor that preserves the human's firing-rate advantage under matching weapon/upgrades. Roles persist through respawn and appear on both sides of larger rosters; ordinary 6v6 has sniper and artillery on opposite sides. All roles use the existing health, damage, pickup and mine systems. Support is an escort behavior and artillery uses existing rockets; V-Tanks' damage-transfer ability and delayed mortar strikes are not ported.

Bots have persistent names such as Iron Jack, Sidewinder and Nitro in the kill feed. Their names survive respawn; the overhead display uses team markers and health/reload bars. The bottom-right controls show drive, aim/fire and zoom. Escape still pauses.

Hit registration follows the visible hull and tracks, including the heavy tank’s longer body. It does not require the shell centerline to pass through the smaller movement collider. Spawn protection and depleted/active shields continue to determine whether a registered hit actually removes hull health.

### Temporary playtest controls and visual feedback

Pause to adjust tank and projectile base speeds independently from 50–200%; 100% is the checked-in speed after the shared 13% increase. The settings persist locally and apply immediately. Mines can be detonated with direct shell hits, even while arming; nearby mines chain and the shooter receives kill credit. All spread pellets originate at the barrel muzzle, with close cover checked before spawning.

Pickup crates carry high-contrast pictograms on every face and emit sparks, a ring and a brief tank glow when collected. Pines use layered boughs, bark and roots, with green foliage and wood splinters on destruction. The 84-name bot pool is shuffled at round start and names persist through respawn.
