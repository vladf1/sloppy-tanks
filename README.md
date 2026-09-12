# Sloppy Tanks

An original, local 3D toy demolition game: one human and eleven bots, blue versus red, a destructible yard, and quick respawns. Built with plain TypeScript, Three.js, Rapier and Howler. No React or UI framework.

## Run

```sh
npm ci
npm run dev
```

Open the Vite URL printed in the terminal (normally http://127.0.0.1:5173/sloppy-tanks/). The performance notebook is at `/sloppy-tanks/benchmark.html`.

```sh
npm run check      # lint, formatting, TypeScript 7 build and regression tests
npm test           # focused simulation regression tests
npm run validate   # ten complete seeded matches and reset checks
npm run build      # strict TypeScript and production bundle
node scripts/browser-check.mjs  # actual Chrome keyboard/mouse smoke test
node scripts/benchmark.mjs      # 1440p Chrome normal, stress, ten resets, 20-minute session
```

The browser scripts use installed Google Chrome and isolated temporary profiles. They do not access the user's existing Chrome profile. The benchmark takes approximately 23 active minutes; sleep or focus pauses extend it. It writes `artifacts/benchmark-results.json`; the notebook reads that file. `artifacts/simulation-results.json` contains accelerated, non-rendered match validation. These are deliberately labeled separately.

For a shorter before/after comparison, run `node scripts/profile.mjs before` on the original revision and `node scripts/profile.mjs after` on the changed revision with Vite running. Each pass records three seeded 20-second normal runs and three stress runs at 1440p, excluding the first five seconds, plus separate Chrome DevTools CPU profiles. Avoid source edits or other browser workloads during measurement. Full results, browser-check output and importable `.cpuprofile` files stay locally in `artifacts/performance/`; the compact comparison is saved to `artifacts/performance-results.json` only after both passes finish without browser errors. The production build also type-checks the profiler's gameplay fixtures. For a different Vite port, prefix either `profile.mjs` or `browser-check.mjs` with `SLOPPY_URL=http://127.0.0.1:5175/sloppy-tanks/`.

The September 7 review fixes queued mine input, random tower alignment and respawn physics, and reduces repeated searches, allocations and wreck preparation. All 82 tests, ten complete simulated matches, browser controls and ten rendered resets pass. Alternating original/updated Chrome runs measured normal render CPU 1.57→1.50 ms/frame and stress 2.63→2.76 ms/frame; stress p99 rose 9.30→9.45 ms. The result is mixed, with no overall speedup claimed. The performance notebook includes the initial three-seed comparison and the alternating repeat; older results remain in VALIDATION.md.

## Static textures

`npm run generate:textures` runs `scripts/generate-textures.ts` using the development-only Node canvas library. It writes nine pickup faces (five power-ups and four ammo crates) to `public/textures/pickups/`, siding and shingles to `public/textures/houses/`, and grayscale armor wear to `public/textures/tanks/armor-wear.png`. The shared 512 × 512 armor map adds visible paint wear, panel seams, scratches and bump detail while retaining each tank’s team color. These PNGs are checked in; local dev/build commands use them directly. Local development and GitHub Pages builds load/copy the saved assets directly; they do not run the generator. Run the offline command and commit the updated images only when changing the artwork or pickup colors. Run `npm run generate:ammo` to regenerate only the four ammunition pictograms from `scripts/generate-ammo-icons.ts`; `node --import tsx scripts/generate-laser-pickup.ts` regenerates the laser-defense pictogram. Crates use shared box/rim/handle geometry with colored shell symbols visible on their sides and lids; non-ammo cubes retain their original artwork. Font rendering can vary across operating systems.

AI-generated grass and dirt are separate: runtime WebPs are in `public/textures/ground/`, with original PNGs and prompts in `assets/texture-sources/`. The offline script does not overwrite those images.

`node --import tsx scripts/generate-conifer-texture.ts` regenerates the transparent needle spray in `public/textures/trees/conifer-spray.png`. Pine, spruce and fir models arrange these sprays into irregular branches. Arena trees shed two branches on their first damage and two more at 35% health; cosmetic branches fall and clear after six seconds. Background trees keep their simpler intact models.

Harbor concrete and steel use 512px WebPs in `public/textures/harbor/` (about 87 KiB combined). Higher-resolution editing sources are also WebP, with generation prompts and conversion commands in `assets/texture-sources/harbor/README.md`; these sources are excluded from the game build.

## Saved sounds

The browser loads eleven shared MP3 files from `public/audio/`: the original `shot.mp3`, `explosion.mp3`, `impact.mp3`, and `pickup.mp3`, four `shot-{spread,rocket,ricochet,piercing}.mp3` variants, a quiet `hit.mp3` confirmation tick, `laser.mp3` for defensive zaps, and a short rising `promotion.mp3` chime for the player's rank upgrades. Spread has a sharp noisy attack, rockets a longer low thump, ricochet a metallic tone, and piercing a brief high snap. Howler handles playback, volume and stereo placement; the browser does not synthesize sounds. These are mono MP3 effects encoded offline from 22,050 Hz PCM using FFmpeg/libmp3lame at VBR quality 2, totaling 24,026 bytes. Player shots remain audible when nearby bots fire at the same time; clustered hit confirmations share an 80 ms sound throttle and defensive zaps a 50 ms throttle.

With FFmpeg installed (`brew install ffmpeg` on macOS), run `npm run generate:audio` offline when changing the sound design in `scripts/generate-audio.ts`, then commit the updated MP3s. Seeded noise makes the output reproducible. Local development and GitHub Pages builds use the saved files directly, without regenerating them.

## Play

- **WASD / Arrow keys**: screen-relative steering. The hull turns gradually toward the requested direction; requesting a direction behind it reverses.
- **Mouse**: independent turret aim.
- **Hold left mouse**: fire.
- **Right click**: drop a mine; 0.8-second arming delay and 7-second cooldown.
- **Q / E**: previous / next stocked ammunition; wraps and skips empty slots.
- **1–5**: Standard, Spread, Rocket, Ricochet, Piercing in HUD order. Empty types leave the selection unchanged. Number-pad keys also work; held-key repeat is ignored.
- **Wheel / two-finger trackpad scroll**: next stocked ammo when scrolling down, previous when scrolling up; wraps and skips empty slots. One change per 120 ms.
- **Shift + wheel / two-finger scroll**: zoom, retaining the 23–52 limits. Mac finger direction follows the Natural scrolling setting.
- **Escape** or **Pause**: pause; resume from the menu.
- Losing focus clears controls and pauses. Hidden tabs stop simulation and rendering.

Choose Skipper (80 HP), Bruiser (100 HP), or Big Rig (140 HP). Standard shells deal 40 damage. The human's team is chosen randomly. Blue uses diamonds; red uses twin bars. Health and reload bars float above vehicles, and the human has a yellow ground ring and a brief spawn pulse.

Team rounds last up to five minutes or end at 100 team kills. A timed tie enters next-kill overtime. Death launches a cosmetic physical wreck with a random tumble axis per part. One in four breakups sends the turret 20–30 units higher, while ordinary arcs stay at 4.5–8 units. Pieces persist through their flight, then clear. Death gives a three-second respawn window with vehicle selection. A respawn has two seconds of protection, cancelled by firing. Spawn selection considers enemy distance, line of sight and friendly congestion.

**Solo Assault** is a one-life survival run with a **ten-minute limit** and unlimited enemy replacements, up to six active at once. The scoreboard shows your credited **KILLS**, active enemies and remaining time; there is no kill target or finite reserve. Dying ends the run immediately; surviving ten minutes completes it. Both outcomes show your final kill count, and a new run starts at zero. Enemies replenish at safe edge positions with at least one second between replacements. The six enemy slots and their render objects are reused, and each replacement starts Rookie, so long runs keep bounded tank/HUD counts. Solo retains its weaker enemy armor, slower firing and reduced enemy damage. Pausing freezes the timer.

Drive through ammunition crates to refill reserves. If the player has no advanced ammunition, the first collected type is selected automatically; later crates preserve the current selection. Standard is unlimited; special ammo comes only from map crates, persists until fired, and is cleared on death, respawn and round reset. Every tank starts with standard selected. The HUD shows all five types, counts, `∞` for standard, dimmed empty slots and a highlighted selection.

| Ammo     | Damage / behavior                                                                          |  Per crate | Carry limit |
| -------- | ------------------------------------------------------------------------------------------ | ---------: | ----------: |
| Standard | 40 damage, one bounce                                                                      |          — |   Unlimited |
| Spread   | Three 27-damage shells; one unit per volley                                                | 18 volleys |          36 |
| Rocket   | 65-damage breaching rocket; accelerates to 2.5× launch speed over one second               |         12 |          24 |
| Ricochet | 80 damage, three bounces                                                                   |         24 |          48 |
| Piercing | Standard damage, speed and cadence; no bounce; intercepts one opposing shell and continues |         24 |          48 |

Eight ammo crates, two per special type, occupy four route pairs mirrored by 180 degrees in Pine Village and randomized maps. They refill after 13 seconds. Tanks at that type's carry limit leave the crate available; partial refills report the actual amount received, such as `+3 ROCKETS`. Ammo types never combine. Selection is applied before firing in each simulation tick. Switching and collection preserve an active reload; only emitted shots consume ammo. The final special shot automatically selects standard, and held fire resumes after the fired weapon's normal cooldown. Selection is ignored while dead or outside play; pause, blur and round transitions clear pending input.

Every unavailable pickup leaves a dim podium with a colored ring that fills over its respawn delay; the floating crate or cube returns when ready. This includes the laser's initial 25-second arrival and its later 45-second respawns. Progress stops while paused. The crosshair dims while reloading and brightens when ready; credited enemy hull damage briefly flashes it white and plays a quiet centered tick, including lethal hits. Cover impacts, self damage and fully protected hits do not confirm. Below 25% health the existing hull panel pulses gently, stops on repair/death, and pauses with the game; reduced-motion settings use a static tint.

Rapid fire halves the selected weapon's firing interval for 20 seconds. Speed adds 50% for 20 seconds. Shield absorbs 120 damage (three standard shells) or expires after 20 seconds; excess damage reaches the hull. Repair fully heals, with four pickups on west, east, north and south routes. Repeated power-ups refresh their timer or shield capacity without multiplying their strength. Mines remain the independent right-click ability. Allies do not take damage or block projectiles. Self-inflicted explosions can kill the owner without awarding a point. Drum and mine chains preserve the initiating damage owner.

**Laser Defense** is a rare automatic point-defense power-up. One cyan pickup appears at the map center after 25 seconds and takes 45 seconds to refill after collection. It lasts twenty seconds, with a **50% chance per incoming enemy projectile** within seven world units. A small turret emitter sends a brief, thin beam to each successful intercept. The laser checks approaching paths near the tank, ignores allies/outgoing shots, and requires clear line of sight. Each projectile gets only one roll per defending tank, including after a miss or bounce; it is not a repeated per-frame probability. Interceptions share the swept collision timeline with cover, hulls, mines and other shells. Successful zaps remove rockets without an explosion, award no kills and leave cannon ammunition/reload alone. The timer pauses with the game and clears on death, respawn or reset. Bots use the same rules. Balance constants are in `LASER_DEFENSE` in `src/game/data.ts`; mechanics and visuals are in `laser-defense.ts` and `laser-visuals.ts`.

The current pacing keeps standard shell speed at 21.696 world units/second and bot weapon interval at 0.85 seconds. The human fires 20% faster (about 0.71 seconds per standard shot); the same advantage applies to special weapons and rapid-fire upgrades. Bots also retain their extra aiming/fire delays. Balanced movement is about 8.95 units/second, adapted from V-Tanks as described below. **There is no camera shake.** Impact feedback comes from recoil, sparks, fragments, light and sound. Surviving a hull hit gives the tank a brief 0.28-second visual jolt, a gold/white spark burst and clear health loss in the overhead bar. This does not move the physics body or shake the camera; fully shield-absorbed hits do not trigger hull-damage feedback.

## Tank veterancy

Every tank, including friendly and enemy bots, starts as a **Rookie**. Deal enemy hull damage to earn **1 XP per actual HP removed**, plus **50 XP for a kill**. Assisting tanks keep the XP from their own damage. Overkill, shield absorption, spawn protection, friendly/self damage, scenery destruction and laser interceptions give no XP. Ranks belong to the current tank's life: respawning starts fresh, while surviving keeps all progress. Delayed mines, shells and explosive chains retain their firing life and cannot promote a replacement tank.

| Rank    | Total XP | Damage | Fire rate | Max hull | Self-repair               |
| ------- | -------: | -----: | --------: | -------: | ------------------------- |
| Rookie  |        0 |   Base |      Base |     Base | None                      |
| Veteran |      200 |   +10% |      +10% |     +10% | None                      |
| Elite   |      500 |   +20% |      +15% |     +15% | 1% of max hull per second |
| Heroic  |    1,000 |   +30% |      +20% |     +20% | 2% of max hull per second |

Bonuses are totals relative to Rookie. Promotion preserves the tank's remaining hull percentage. Damage bonuses apply to fired ammunition and laid mines, using rank at the time of firing/placement; secondary scenery blasts and shell-interception blasts retain their usual damage. Fire-rate bonuses affect both cannon reload and the bots' extra firing delays, and combine with rapid fire. Self-repair begins after five seconds without firing, laying a mine or taking damage (including shield hits). It pauses with the game and cannot exceed the upgraded max hull; repair pickups also respect that maximum. Solo enemies retain their difficulty scaling.

One to three small gold chevrons appear beside promoted tanks' health bars. The compact player panel shows the current rank beside the tank name, without an XP bar or point counter; hovering over the rank explains its bonuses. Promotions produce a brief gold pickup-style glow, a player toast and a saved chime. Balance values and XP/repair rules live in `src/game/veterancy.ts`. The Veteran/Elite/Heroic progression is inspired by [EA's published Generals Zero Hour veterancy tiers](https://github.com/electronicarts/CnC_Generals_Zero_Hour/blob/main/GeneralsMD/Code/GameEngine/Include/Common/GameCommon.h); these XP thresholds and bonuses are tuned for this game.

## Destruction and navigation

Timber panels and walls break independently. Drums explode. Each tower has one authored support-health object; removing it creates a burst of prebuilt fragments and two persistent side-rubble colliders, leaving its middle traversable. Foundations and permanent barriers survive the round. Flying debris and wrecks collide with ground and cover, but cannot damage or trap living tanks.

The 48 × 48 navigation grid has conservative clearance for vehicles and cardinal A* paths. Destruction rebuilds the affected cells immediately. Bots replan on topology changes and use Rapier queries for visibility and nearby vehicles. Their staggered decisions choose combat, pickups, retreat or flanks; reaction time, aim error, brief memory and congestion recovery constrain them. There is no special human-target priority.

## Code map

Start with `src/main.ts`, which wires input, fixed simulation steps, rendering, audio and the DOM. See [CONTRIBUTING.md](CONTRIBUTING.md) for the coding conventions and validation commands.

| Responsibility                                   | Files under `src/`                                                                                              |
| ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| Application loop and performance recording       | `main.ts`, `diagnostics.ts`                                                                                     |
| Simulation state, tick order and match lifecycle | `game/simulation.ts`, `game/match.ts`                                                                           |
| Tank spawning, respawning and tracked movement   | `game/tank-lifecycle.ts`, `game/tank-driving.ts`                                                                |
| Weapon firing and ordered projectile contacts    | `game/weapons.ts`, `game/projectiles.ts`, `game/hitboxes.ts`, `game/laser-defense.ts`                           |
| Mines, pickups, damage and experience            | `game/mines.ts`, `game/pickups.ts`, `game/damage.ts`, `game/ammunition.ts`, `game/veterancy.ts`                 |
| Bot strategy, personalities, steering and paths  | `game/ai.ts`, `game/bot-strategy.ts`, `game/bot-personalities.ts`, `game/bot-movement.ts`, `game/navigation.ts` |
| Authored balance and shared rules                | `game/data.ts`, `game/combat-rules.ts`, `game/simulation-rules.ts`, `game/view-settings.ts`                     |
| Arena layout and deterministic math              | `game/arena.ts`, `game/math.ts`                                                                                 |
| Scene synchronization and static environment     | `game/presentation.ts`, `game/scenery.ts`                                                                       |
| Geometry and shared mesh factories               | `game/tank-model.ts`, `game/cover-model.ts`, `game/wreck-model.ts`, `game/model-primitives.ts`                  |
| Physical debris and cosmetic effects             | `game/fragments.ts`, `game/wrecks.ts`, `game/particle-effects.ts`, `game/tracks.ts`, `game/*-visuals.ts`        |
| World-space HUD and GPU resource ownership       | `game/tank-bars.ts`, `game/reticle.ts`, `game/render-resources.ts`                                              |
| DOM controller, markup, input and audio          | `game/ui.ts`, `game/ui-markup.ts`, `game/controls.ts`, `game/audio.ts`                                          |
| Texture loading and UV mapping                   | `game/*-surfaces.ts`                                                                                            |
| Entity and command contracts                     | `game/types.ts`                                                                                                 |

`models.ts` and the combat re-exports in `weapons.ts` preserve the entry points used by existing preview pages and regression harnesses. New implementation code should import the module that owns the behavior.

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

Pine Village now sits in a greener mountain valley with a flowing creek, two arched timber footbridges, a turning watermill, a loaded log cart, a village sign, wooded hills and a snow-tipped ridge. Low grass sways in the breeze, wildflowers grow in the meadows, cottage windows have planted boxes, and permanent chimneys release soft smoke. The larger landmarks and creek remain outside the arena; the cover layout, pickup access and destruction rules are unchanged. Geometry, instancing and small shaders reuse the existing texture files; scenery is created on first use and retained across resets. `tests/village.browser.html` provides close-up views and a map-reset resource check.

Tank silhouettes now follow the supplied examples: tall cast or angular turrets, thick gray guns, broad dark tracks and prominent gray armor panels. Projectiles use distinct compact models with blue/red team markings: a pointed standard shell, round spread pellets, a rocket with a nose cone and four swept fins, a spinning hexagonal ricochet puck, and a slim cyan piercing dart. Rockets have a short flickering exhaust. The rocket body is approximately 0.94 world units long, matching the previous stretched-sphere rocket; the standard shell is 0.70 units long. All five bodies are at most 1.0 unit long and 0.52 units wide, with shape and markings providing the distinction. `src/game/projectile-visuals.ts` builds and batches the geometry once, retaining the 600-projectile display cap; it does not change projectile physics, damage, speed or hitboxes.

The outlined aiming reticle stays visible over terrain and cover. At the earlier redesign revision, shell speeds were reduced by 20%; the current standard/ricochet/piercing speed is 21.696, spread 19.888 and rocket 15.368 units/second after later shared tuning. Rapid fire changes cadence rather than shell speed; lifetime increases to 3.5 seconds to preserve travel range. The player-centered camera and reduced bot accuracy/cadence are retained.

## Movement, breakup and quick selection

Clicking a vehicle card starts immediately. The selector displays rounded road speeds: Skipper 35 km/h, Bruiser 29 km/h, Big Rig 22 km/h. Simulation uses the precise speeds below.

Destroyed tanks separate into actual hull and turret models. In 40% of breakups the barrel detaches too; otherwise it stays on the spinning turret. Planned landing separation is roughly 14–28 world metres, constrained by arena and visible-view margins. One in four breakups launches the turret 20–30 metres above its starting height; ordinary arcs rise 4.5–8 metres. Each piece gets an independent, uniformly random tumble axis and spin speed. Physical collisions can shorten or redirect a throw. High launches may outlast the three-second respawn; pieces clear after their planned flight plus 1.8 seconds, within the shared 80-piece cap. The compact respawn strip leaves the effect visible. Pieces remain cosmetic and cannot damage or obstruct living tanks.

## Publishing

Play at https://fridman.me/sloppy-tanks/. Every push to `main` runs the tests and production build in GitHub Actions, then deploys `dist/` to GitHub Pages after they succeed. You can also run the workflow manually from the Actions tab.

The Vite base path is `/sloppy-tanks/`. GitHub Pages inherits `fridman.me` from the account site.

## V-Tanks movement, upgrades and tracks

Reference: [V-Tanks](https://fridman.me/v-tanks/), verified against `vladf1/v-tanks` revision `570bf8dd46a48c0761a4faccafa40197a821267a`. Its balanced tank travels at 184 source units/second and its standard shell at 535. Scaling that ratio to our 19.2 m/s shell and adding the requested 20% base-speed increase gives 7.924 m/s; light and heavy use its 1.24 and 0.76 class multipliers (9.826 and 6.022 m/s). A further shared 13% speed increase brings light/balanced/heavy to 11.103 / 8.954 / 6.805 m/s and standard/spread/rocket projectiles to 21.696 / 19.888 / 15.368 m/s. Acceleration/braking is 100 m/s² and hull rotation is capped at 3.5 radians/second (about 0.45 seconds for a 90° turn). Movement follows the hull, with reduced drive during sharp turns and automatic reverse at 80% of forward speed when the requested direction is behind the tank. Releasing movement brakes promptly; mouse aiming stays independent. Players and bots share this steering model. This adapts the dodge timing and responsive handling to our 3D arena; screen-space speed still depends on zoom.

Opposing shells intercept continuously, including between simulation ticks and after ricochets. Allied shells pass through each other. The earliest wall, tank, expiry or shell contact wins; thin cover blocks interception. Ordinary interceptions remove both shells with a small blast that deals one standard 40-damage hit to nearby tanks on either team, credited to the opposing shell's shooter. Ordinary blast radius is 3 m; intercepted rockets use 5.3 m. This blast does not damage cover or trigger mines. A fresh piercing shell instead destroys the opposing shell and continues with its one interception allowance spent, producing a small impact without blast damage, including against rockets. Two fresh piercing shells both continue with their allowances spent; that pair is resolved only once, even if still touching next tick. Later contacts use normal interception rules. Piercing stops on tanks and cover.

All moving tanks leave paired tread impressions following their hull heading. Marks are distance-spaced, fade progressively from 4 to 18 seconds, and use one instanced draw call capped at 8,192 treads. A full buffer skips new impressions until the oldest pair has completely faded; visible marks are never overwritten abruptly. Tracks freeze during pause, clear with a new round, and are cosmetic. Camera shake remains absent.

## Bot personalities

Adapted from the local V-Tanks enemy profiles:

- **Scout:** fast approach, close fighting range and loose aim.
- **Guard:** holds medium range, retreats when crowded and strafes across firing lanes.
- **Sniper:** moves into a long firing lane, stops to aim, and retreats from close threats.
- **Heavy:** slow advance with a deliberate firing rhythm.
- **Minelayer:** closes in and deliberately drops mines near opponents.
- **Support:** escorts nearby teammates and fights from farther back.
- **Artillery:** takes a distant position and prefers finite breaching rockets collected from crates; uses standard when special stock is empty.

Every tenth bot slot is an aggressive **Hunter** variant (one of eleven bots in a normal round). Hunters pursue through cover using navigation, close to short range and turn faster, but still need line of sight to fire. Personality reloads retain a floor that preserves the human's firing-rate advantage under matching weapon/upgrades. Roles persist through respawn and appear on both sides of larger rosters; ordinary 6v6 has sniper and artillery on opposite sides. All roles use the existing health, damage, pickup and mine systems. Support is an escort behavior and artillery uses existing rockets; V-Tanks' damage-transfer ability and delayed mortar strikes are not ported.

Bots keep a patrol destination until arrival, retain a useful crate while approaching it, and prefer their current visible enemy unless another is substantially closer. Route following looks several clear grid waypoints ahead and brakes near the destination. Before moving, bots sweep their actual hull against cover and other tanks, choose an open side and hold that direction briefly. This replaces the retreat-toward-target flip and competing separation forces that caused twitching and head-on deadlocks. Lack of movement for 1.2 seconds starts a committed local detour that combat decisions cannot immediately overwrite; respawn clears that recovery state. Random-map patrol goals inside cover move to a navigable neighboring cell. Existing aiming error, reaction time, role ranges, fire cadence and easy-mode damage remain in effect.

Bots seek useful crates and skip full reserves. Artillery/heavy prefer rockets, scouts/minelayers spread, snipers piercing, and guards/support ricochet. They use another stocked special when their preferred type is empty, and standard for routine cover clearing or empty special reserves. The generous initial supply and carry limits live beside the weapon stats in `src/game/data.ts`; cycling/refill rules live in `src/game/ammunition.ts`.

Bots have persistent names such as Iron Jack, Sidewinder and Nitro in the kill feed. Their names survive respawn; the overhead display uses team markers and health/reload bars. The bottom-right controls show drive, aim/fire and keyboard ammunition shortcuts. Start and pause menus share the full controls, including scroll selection and Shift-scroll zoom. Escape still pauses.

Hit registration follows the visible hull and tracks, including the heavy tank’s longer body. It does not require the shell centerline to pass through the smaller movement collider. Spawn protection and depleted/active shields continue to determine whether a registered hit actually removes hull health.

### Temporary playtest controls and visual feedback

Pause to adjust tank and projectile base speeds independently from 50–200%; 100% is the checked-in speed after the shared 13% increase. The settings persist locally and apply immediately. Mines can be detonated with direct shell hits, even while arming; nearby mines chain and the shooter receives kill credit. All spread pellets originate at the barrel muzzle, with close cover checked before spawning.

Pickup crates carry high-contrast pictograms on every face and emit sparks, a ring and a brief tank glow when collected. Pines use layered boughs, bark and roots, with green foliage and wood splinters on destruction. The 84-name bot pool is shuffled at round start and names persist through respawn.

## Ammunition validation

`npm test` covers inventories, consumption/caps, cooldown protection, selection and input clearing, crate contention/refill, each ammunition type, piercing contact ordering/ownership/cover, role preferences and pickup routes on both authored maps. `npm run validate` runs ten seeded full matches and checks reset body counts. `npm run build` checks TypeScript and produces the release assets.

With Vite running, use `SLOPPY_URL=http://127.0.0.1:5179/sloppy-tanks/ node scripts/ammunition-check.mjs` (substitute the port Vite reports). It drives real wheel/Shift-wheel and pointer input, captures the production crate/HUD at desktop and narrow sizes, and measures three seeds each of standard vs plentiful spread/piercing combat with 24 tanks and rapid fire. Results are saved in `artifacts/ammunition-results.json`; screenshots stay under ignored `artifacts/performance/ammunition/`. Add `--visual-only` to repeat input and screenshot checks without rerunning performance or replacing its results.

## Bot movement validation

`SLOPPY_URL=http://127.0.0.1:5173/sloppy-tanks/ node scripts/driving-check.mjs` checks real WASD and arrow-key input through the rendered application loop: forward travel, automatic reverse, gradual turns, release braking and pause. Substitute the live Vite port. Screenshots and measurements are saved under ignored `artifacts/performance/driving/`.

`node --import tsx scripts/bot-movement-check.ts after` repeats three controlled movement cases and six 90-second village/harbor runs. `before` is reserved for capturing a baseline before changing the controller. Reports live in `artifacts/bot-movement-before.json` and `artifacts/bot-movement-after.json`. A measured stall is a two-second window with movement requested for more than 80 of 120 ticks but under one metre of net displacement; stationary firing roles are excluded. The detector also records direction reversals greater than 120 degrees and stalls with at least eight such reversals. These are repeatable regression indicators, not a guarantee that every pause in gameplay is a bug.

`SLOPPY_URL=http://127.0.0.1:5179/sloppy-tanks/ node scripts/bot-movement-browser.mjs` verifies the retreat-at-wall and head-on scenarios through the real rendering loop, then records a 24-tank match on a Surprise me selection. Substitute the live Vite port. Results are in `artifacts/bot-movement-browser.json`, with screenshots under ignored `artifacts/performance/bot-movement/`.

## Difficulty and combat help

Choose Easy, Normal or Hard on the start screen. Difficulty stays fixed for the round; return to the start screen to choose it for a new round. The choice is saved locally and applies to Team Battle and Solo Assault. Normal preserves the original balance, including Solo's existing enemy adjustments. Easy gives opposing bots 1.2× reaction delay, 1.2× aim error, 1.1× firing delay and 90% damage; Hard uses 0.7× reaction delay, 0.65× aim error, 0.85× firing delay and 115% damage. Allied bot behavior, tank health and human weapons retain their original settings.

Click the ammo buttons or use 1–5, Q/E or scrolling. Hover tooltips explain each type. The compact panel keeps shortcuts, ammo counts, hull health and mine status visible. Active-effect labels and empty-ammo notices stack above it without resizing it. Empty direct selections explain that a crate is needed. Firing the last special round announces the automatic return to unlimited standard shells.

A brief red chevron around the player points toward incoming hull damage, using the shell's incoming direction (including ricochets) or explosion location. Death feedback names the weapon or hazard and its credited initiator, including self-inflicted damage, and stays on the respawn or Solo results screen. Spawn protection and fully absorbed shield hits do not produce hull-damage indicators.

For repeatable checks in the built-in browser, open `/sloppy-tanks/tests/usability.browser.html` on the local Vite server and click **Run checks**. This development-only page controls application frames and provides ammo/damage fixtures for inspecting the real HUD. `tests/player-usability.test.ts` covers difficulty, ammo notices and combat-source attribution as part of `npm test`.

### Harbor Havoc

Choose **Harbor Havoc** in the map selector for a sunset container port, available in Team Battle and Solo Assault. Steel containers form permanent cover; wooden cargo stacks break apart to open shortcuts. Crates develop splintered cracks after a hit, then lifted lid boards and a broken strap at 35% health or less. Damage patterns vary by crate and stay consistent as the damage deepens. Wide dockside lanes flank the central loading yard, with weathered concrete and painted steel surfaces. Three detailed container ships, four gantry cranes with swaying lifting gear, forklifts, mooring lines, and animated water surround the arena. The water uses continuous ripples, sky and sun highlights, shallow-water color, and foam along the quay; all ships and dock machinery stay outside the arena walls.

Use `?map=harbor` on the game URL to open with Harbor Havoc selected. For development, `tests/harbor.browser.html` previews the docks and cargo and checks scenery switches and GPU resource reuse.

Surprise me picks Pine Village or Harbor Havoc for each new match, using the complete authored layout and scenery. The choice stays fixed during play and pause, and the results show the chosen map’s name. Available maps are listed once in `src/game/maps.ts`, shared by the selector and the shuffle. `?map=surprise` opens this option; old `?map=random` links also use it.
