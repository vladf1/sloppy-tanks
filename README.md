# Sloppy Tanks

An original, local 3D toy demolition game: one human and eleven bots, blue versus red, a destructible yard, and quick respawns. Built with plain TypeScript, Three.js, Rapier and Howler. No React or UI framework.

## Run

```sh
npm ci
npm run dev
```

Open the Vite URL printed in the terminal (normally http://127.0.0.1:5173). The performance notebook is at `/benchmark.html`.

```sh
npm test           # focused simulation regression tests
npm run validate   # ten complete seeded matches and reset checks
npm run build      # strict TypeScript and production bundle
node scripts/browser-check.mjs  # actual Chrome keyboard/mouse smoke test
node scripts/benchmark.mjs      # 1440p Chrome normal, stress, ten resets, 20-minute session
```

The browser scripts use installed Google Chrome and isolated temporary profiles. They do not access the user's existing Chrome profile. The benchmark takes approximately 23 active minutes; sleep or focus pauses extend it. It writes `artifacts/benchmark-results.json`; the notebook reads that file. `artifacts/simulation-results.json` contains accelerated, non-rendered match validation. These are deliberately labeled separately.

## Play

- **WASD**: screen-relative movement.
- **Mouse**: independent turret aim.
- **Hold left mouse**: fire.
- **Right click**: drop a mine; 0.8-second arming delay and 7-second cooldown.
- **Wheel**: zoom.
- **Escape** or **Pause**: pause; resume from the menu.
- Losing focus clears controls and pauses. Hidden tabs stop simulation and rendering.

Choose Skipper (80 HP), Bruiser (100 HP), or Big Rig (140 HP). Standard shells deal 40 damage. The human's team is chosen randomly. Blue uses diamonds; red uses twin bars. Health and reload bars float above vehicles, and the human has a yellow ground ring and a brief spawn pulse.

Rounds last up to five minutes or end at 50 team kills. A timed tie enters next-kill overtime. Death launches a cosmetic physical wreck and gives a three-second respawn window with vehicle selection. A respawn has two seconds of protection, cancelled by firing. Spawn selection considers enemy distance, line of sight and friendly congestion.

Pickups are collected by driving through them. Rapid fire, spread, breaching rockets and enhanced ricochet replace the current weapon for 14 seconds. Shield and speed refresh rather than stack; repair restores up to 65 HP. Mines are independent of weapon pickups. Allies do not take damage or block projectiles. Self-inflicted explosions can kill the owner without awarding a point. Drum and mine chains preserve the initiating damage owner.

The current pacing incorporates play feedback: standard shell speed is 24 world units/second, balanced movement is 6.5 units/second, and standard firing interval is 0.85 seconds. **There is no camera shake.** Impact feedback comes from recoil, sparks, fragments, light and sound.

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

Physics runs at 60 Hz with at most five catch-up steps. Dynamic live tanks lock roll and pitch, using acceleration-limited impulses that preserve external knockback. Death unlocks rotation and retains momentum. Projectiles use swept Rapier rays; damage is never inferred from rendered meshes. Human and bot controllers produce the same `VehicleCommand`. Stable IDs, a seed and a read-only snapshot provide a future command/snapshot boundary; networking and replay synchronization are not implemented.

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
