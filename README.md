# Sloppy Tanks

A browser tank game with destructible cover, team battles and solo survival. Built with TypeScript, Three.js, Rapier and Howler.

## Run

Use Node.js 24 or newer:

```sh
npm ci
npm run dev
```

Open the URL Vite prints, normally `http://127.0.0.1:5173/sloppy-tanks/`.

## Play

Choose Skipper, Bruiser or Big Rig, then select a map and difficulty:

- **Team Battle:** six versus six, with respawns. First to 100 kills or the highest score after five minutes wins; a timed tie enters next-kill overtime.
- **Solo Assault:** survive ten minutes on one life against up to six enemies at once, with unlimited replacements.
- **Maps:** Pine Village, Harbor Havoc and Dusty Dig. **Surprise me** chooses among all three authored maps.
- **Difficulty:** Easy, Normal or Hard; fixed for the round and saved locally.

Map links accept `?map=village`, `?map=harbor`, `?map=quarry` or `?map=surprise`.

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

Losing focus clears input and pauses the round. Standard ammunition is unlimited; collect crates for special ammunition. Power-ups provide rapid fire, speed, shields, repairs and automatic laser defense. Enemy hull damage and kills earn Veteran, Elite and Heroic ranks; death resets rank progress. Timber and cargo break apart, drums explode, and destroyed cover opens routes. Allied tanks block shells without losing hull health or shields; rockets detonate on contact, with their existing self-damage rule. Bots hold fire when an ally blocks a firing lane, including spread pellets.

## Development

```sh
npm run check      # lint, formatting, TypeScript, production build and tests
npm test           # simulation and behavior regression tests
npm run validate   # ten seeded full matches and reset checks
npm run build      # production output in dist/
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for conventions, browser checks and performance measurements. The performance notebook is available at `/sloppy-tanks/benchmark.html`.

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

## Assets

Runtime textures, tank previews and sounds are checked in under `public/`. Development and production builds use these files directly. Regenerate them only when changing artwork or sound:

| Command                     | Output / requirements                                                                                  |
| --------------------------- | ------------------------------------------------------------------------------------------------------ |
| `npm run generate:textures` | Procedural pickup, house, barrel and armor artwork, followed by texture optimization; requires `cwebp` |
| `npm run optimize:textures` | Six optimized runtime textures from preserved sources; requires `cwebp`                                |
| `npm run generate:ammo`     | The four special-ammunition pictograms; requires `cwebp`                                               |
| `npm run generate:previews` | Tank selection WebPs rendered from the actual models; requires Google Chrome                           |
| `npm run generate:audio`    | Eleven MP3 effects; requires FFmpeg                                                                    |

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

The build separates the interactive menu from gameplay, graphics, physics and audio dependencies. A blue HTML loading screen fades into the menu, which stays usable while the engine, textures, audio and a hidden arena prepare. Shader compilation runs without drawing a 3D menu background. GO reuses the prepared arena when its choices still match, or builds the newly selected round; an early GO waits in the menu. Rapier's WASM is emitted as a separate hashed file and preloaded from HTML. Hosts should serve it as `application/wasm` with gzip or Brotli compression.
