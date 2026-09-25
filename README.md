# Sloppy Tanks

A browser tank game with destructible cover, team battles and solo survival. Built with TypeScript, Three.js, Rapier and Howler.

## Run

Use Node.js 24 or newer:

```sh
npm ci
npm run dev
```

Open the URL Vite prints, normally `http://127.0.0.1:5173/sloppy-tanks/`.

Rendering requires WebGPU, HTTPS or localhost, and a browser/GPU that supports it. There is no WebGL fallback. **Stats for nerds** shows rendering diagnostics.

## Play

Choose Skipper, Bruiser or Big Rig, then select a map and difficulty:

- **Team Battle:** six versus six, with respawns. First to 100 kills or the highest score after five minutes wins; a timed tie enters next-kill overtime.
- **Solo Assault:** survive ten minutes on one life against up to six enemies at once, with unlimited replacements.
- **Maps:** Pine Village, Harbor Havoc and Dusty Dig.
- **Difficulty:** Easy, Normal or Hard; fixed for the round and saved locally.

Map links accept `?map=village`, `?map=harbor` or `?map=quarry`.

| Control            | Action                                                   |
| ------------------ | -------------------------------------------------------- |
| WASD / arrows      | Steer toward a screen direction; opposite input reverses |
| Mouse              | Aim the turret                                           |
| Hold left click    | Fire                                                     |
| Right click        | Drop a mine                                              |
| 1–5 / ammo buttons | Select Standard, Spread, Rocket, Ricochet or Piercing    |
| Q / E / scroll     | Cycle stocked ammunition                                 |
| Shift + scroll     | Zoom                                                     |
| Escape / Pause     | Pause                                                    |

Losing focus clears held input, while a hidden page pauses the round. Standard ammunition is unlimited; collect crates for special ammunition. Power-ups provide rapid fire, speed, shields, repairs and automatic laser defense. Enemy hull damage and kills earn Veteran, Elite and Heroic ranks; death resets rank progress. Timber and cargo break apart, drums explode, and destroyed cover opens routes. Allied tanks block shells without losing hull health or shields; rockets detonate on contact, with their existing self-damage rule. Bots hold fire when an ally blocks a firing lane, including spread pellets.

On iPads and touchscreens, two thumb sticks appear automatically. Drag the left stick to drive (shorter drags move slower); drag the right stick to aim and push beyond its yellow ring to fire. Release to stop firing while keeping your aim direction. Tap ✹ to drop one mine, tap an ammo slot to select it, and use − / + to zoom. The mine button shows its cooldown. Pause with Ⅱ to choose **Touch controls: Auto / On / Off**; the preference is saved. Landscape gives the clearest view, and portrait is supported. The joystick UI and its styles load only when touch controls are enabled; desktop Auto and Off skip their downloads and hidden UI updates.

Team-only HUNTER Humvees make hit-and-run TOW attacks. They prefer isolated targets, plan an escape before firing, and withdraw behind cover (or open distance when no cover is available). After reloading and a short pause, they approach from a different position. They remain lightly armored and do not escort the player. They stop for 0.9 seconds to aim and stay exposed for 0.65 seconds after launch; a TOW deals 75 base damage. Most Humvee kills erupt in a fireball and tumble as a whole vehicle; roughly one in five instead leaves a quietly smoking wreck with a small hop.

Choose **END BATTLE** from the pause menu to finish early and see your current stats without declaring a winner. The game-over screen is a battle report: eliminations, busiest rolling minute, longest life, best killing spree, damage dealt, highest rank, average kills per minute, direct projectile hit rate, five-second multikills, low-hull kills, revenge, previous-life ordnance kills, mine kills, demolition, pickups, hull damage taken, and shield damage absorbed. Earned callouts celebrate feats such as ONE-TANK ARMY and DEAD BUT DANGEROUS. Survival time excludes pauses; direct hit rate excludes splash-only hits and counts spread pellets individually. Personal bests are saved in this browser separately for each mode, map, and difficulty. PLAY AGAIN immediately starts another round with your current settings; BATTLE SETUP returns to the menu.

## Development

```sh
npm run check          # lint, formatting, TypeScript, production build and tests
npm test               # simulation and behavior regression tests
npm run validate       # ten seeded full matches and reset checks
npm run build          # production output in dist/
npm run lint:fix       # safe ESLint fixes
npm run format         # Prettier for source, tests, scripts, styles and docs
npm run check:browser  # browser checks against a running dev server (set SLOPPY_URL)
```

[AGENTS.md](AGENTS.md) is the development guide: code conventions and the simulation, determinism and rendering rules. [scripts/README.md](scripts/README.md) lists the browser checks and performance measurements. The performance notebook is available at `/sloppy-tanks/benchmark.html`.

| Area                           | Starting points                                                                                    |
| ------------------------------ | -------------------------------------------------------------------------------------------------- |
| Startup and game loop          | `src/main.ts`, `src/game.ts`                                                                       |
| Simulation and match lifecycle | `src/game/simulation.ts`, `src/game/match.ts`                                                      |
| Driving, weapons and damage    | `src/game/tank-driving.ts`, `src/game/weapons.ts`, `src/game/projectiles.ts`, `src/game/damage.ts` |
| Bots and navigation            | `src/game/ai.ts`, `src/game/bot-strategy.ts`, `src/game/bot-movement.ts`, `src/game/navigation.ts` |
| Maps and scenery               | `src/game/maps.ts`, `src/game/presentation.ts`, `src/game/scenery.ts`                              |
| Models and shared resources    | `src/game/tank-model.ts`, `src/game/cover-model.ts`, `src/game/render-resources.ts`                |
| Balance and progression        | `src/game/data.ts`, `src/game/combat-rules.ts`, `src/game/difficulty.ts`, `src/game/veterancy.ts`  |
| Controls, UI and sound         | `src/game/controls.ts`, `src/game/ui.ts`, `src/game/audio.ts`                                      |

The development build exposes `window.sloppy` for diagnostics; `?tweak` opens the development-only zoom panel. `?autoplay` assigns bot controls to the player slot.

Multiplayer is available on the [dev site](https://sloppy-tanks-dev.fridman.me/?multiplayer).
Choose **Play with friends** to browse open rooms. Your saved name is prefilled;
first-time players get a random bot name they can edit. **Auto** picks the team
with fewer human seats (including reconnect reservations). Choose a room and
**Join**, or pick a level and **Create room** to start playing immediately.
Friends can join later through the list or **COPY ROOM LINK** in the menu.
Listings show human player counts, map, bot mode, round time and score.

New rooms default to **Humans only (no bots)**. Empty seats stay empty, and
paused or disconnected players remain idle and vulnerable. Uncheck it when
creating or between rounds to fill the six-versus-six teams with bots; bots
also drive absent humans in that mode. Up to eight people can join, with six
human seats per team. Reconnect within 30 seconds to reclaim the same seat.

The last explicit departure removes the room and frees its simulation. An
accidental disconnect hides an empty room from the list while retaining the
30-second reconnect grace. The host can end a round or choose settings for the
next round. Accounts and saved matches are not required; a server restart ends
the current match.

**Stats for nerds** is available during multiplayer battles: click the bottom-right
button or press **N**. Network rows show RTT, received update count/rate, update
age, server tick and input sequence sent/acknowledged. A received update is one
full-state message or snapshot batch; an input acknowledgement confirms the
server processed an input sequence. Render includes GPU geometries and textures.

Single-player downloads no multiplayer code and opens no game-server connection.
Multiplayer loads its client and UI only on entry and does not run browser physics.
The [plan](docs/multiplayer-plan.md) records remaining playtest gates and the
[server guide](server/README.md) describes local development and deployment. The dev
site's multiplayer runs on a stand-alone Node server on a VPS, with rooms held in
memory.

## Assets

Runtime textures, tank previews and sounds are checked in under `public/`. Development and production builds use these files directly. Regenerate them only when changing artwork or sound:

| Command                         | Output / requirements                                                                                  |
| ------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `npm run generate:textures`     | Procedural pickup, house, barrel and armor artwork, followed by texture optimization; requires `cwebp` |
| `npm run optimize:textures`     | Six optimized runtime textures from preserved sources; requires `cwebp`                                |
| `npm run generate:ammo`         | The four special-ammunition pictograms; requires `cwebp`                                               |
| `npm run generate:pickup-atlas` | The shared pickup atlas from the individual icons; also run by `generate:textures` and `generate:ammo` |
| `npm run generate:previews`     | Tank selection WebPs rendered from the actual models; requires Google Chrome                           |
| `npm run generate:audio`        | Eleven MP3 effects; requires FFmpeg                                                                    |

On macOS, install the offline encoders with `brew install webp ffmpeg`.

Source images and encoding guidance live in [assets/texture-sources](assets/texture-sources/README.md), including [harbor](assets/texture-sources/harbor/README.md), [quarry](assets/texture-sources/quarry/README.md) and [tree](assets/texture-sources/trees/README.md) notes. The optimizer uses 768px grass and sandstone, 512px dirt and concrete, and 512px armor wear and conifer foliage. Source artwork is outside the deployed directory.

For conifer artwork, run `node --import tsx scripts/generate-conifer-texture.ts` followed by `npm run optimize:textures`. Other tree patterns use `node --import tsx scripts/generate-tree-textures.ts`; the laser pictogram uses `node --import tsx scripts/generate-laser-pickup.ts`.

See [tank references](assets/tank-references.md) for model provenance and [water texture notes](public/textures/water/README.md) for its source and license.

## Deployment

Two independent workflows publish on pushes to `main`, after `npm run check` passes:

- **GitHub Pages:** `npm run build` produces `dist/` with the default `/sloppy-tanks/` base for <https://fridman.me/sloppy-tanks/>.
- **Cloudflare Pages:** `npm run build:cloudflare` produces `dist-cloudflare/` with the `/` base for <https://sloppy-tanks.fridman.me/>. The Pages project is `sloppy-tanks`, with <https://sloppy-tanks.pages.dev/> as its provider URL.

`DEPLOY_BASE` controls both Vite asset URLs and the physics preload. The Cloudflare build uses its own output directory and leaves `dist/` intact. Its workflow requires the GitHub Actions secret `CLOUDFLARE_API_TOKEN`, scoped to Cloudflare Pages:Edit on the deployment account. Never commit the token.

For a manual Cloudflare deployment with authenticated Wrangler:

```sh
npm run build:cloudflare
wrangler pages deploy dist-cloudflare --project-name sloppy-tanks --branch main
```

The Namecheap CNAME `sloppy-tanks` points to `sloppy-tanks.pages.dev`; the apex, `www`, and existing GitHub Pages configuration remain separate. To stop the experiment, disable the Cloudflare workflow and remove only that subdomain's CNAME and Pages custom-domain association.

The build separates the interactive menu from gameplay, graphics, physics and audio dependencies. Battle Setup appears immediately with a progress strip while the engine, textures and hidden arena prepare. Pressing GO early changes the button to WAIT and confirms that the round will start automatically; there is no need to keep clicking. The arena's shaders and first frame are prepared before combat starts. GO reuses the prepared arena when its choices still match, or prepares the newly selected map while keeping the menu visible. Independent image downloads, device setup and GPU pipeline compilation overlap where possible. Rapier's WASM is emitted as a separate hashed file and preloaded from HTML. Hosts should serve it as `application/wasm` with gzip or Brotli compression.

### Local dev deployment

`npm run deploy:dev` runs the normal checks, builds the current local checkout
(including uncommitted changes), and publishes to the separate `sloppy-tanks-dev`
Cloudflare Pages project. Install the Wrangler CLI and run `wrangler login` first
(or provide a Pages:Edit API token). The publisher sets the account, project and
`main` deployment branch itself, independent of the local Git branch. No Git
commit or push is required. `npm run build:dev` builds without publishing.
The dev build always connects **Play with friends** to the multiplayer server
on the VPS, and `deploy:dev` redeploys that server from the same checkout first
(over SSH) so client and server versions match.

The dev game is at <https://sloppy-tanks-dev.fridman.me/> and the directory of
browser test pages is at <https://sloppy-tanks-dev.fridman.me/test-pages.html>.
The provider URL is <https://sloppy-tanks-dev.pages.dev/>. The `/build-info.json`
endpoint records the UTC build time, commit, and whether local changes were present.
The build goes to `dist-dev/` with the `/` base; production builds and deployments
stay separate. The Namecheap CNAME `sloppy-tanks-dev` points to
`sloppy-tanks-dev.pages.dev`; Cloudflare must associate a custom domain before its
CNAME changes, and all other DNS records stay as they are.
