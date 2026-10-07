# Sloppy Tanks

A browser tank game with destructible cover, team battles and solo survival. The engine is Rust compiled to WebAssembly: the simulation runs on Rapier, a custom renderer draws handwritten WGSL through WebGPU (`wgpu`), or WebGL2 (`glow`) where WebGPU is unavailable, and a TypeScript page shell handles menus, input and Howler audio. Multiplayer rooms run the same Rust simulation in a native Rust server.

## Run

Use Node.js 24 or newer, [pnpm](https://pnpm.io/installation) 11 or newer, and Rust through [rustup](https://rustup.rs). On macOS:

```sh
brew install node pnpm rustup   # once per machine
rustup toolchain install        # in the checkout: the toolchain rust-toolchain.toml pins
cargo install wasm-bindgen-cli --version 0.2.129 --locked
pnpm install                    # once per checkout or worktree, and after dependency changes
pnpm run wasm                   # build the engine; rerun after Rust or WGSL edits
pnpm run dev
```

`rust-toolchain.toml` pins the Rust release, rustfmt, clippy and the
`wasm32-unknown-unknown` and `x86_64-unknown-linux-musl` targets. The wasm-bindgen CLI must match the `wasm-bindgen` crate pin
(0.2.129). Homebrew's `rust` formula provides a `cargo` that ignores
`rust-toolchain.toml`, so rustup's must come first on `PATH`:

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
```

On other systems, `npm install -g pnpm` also works. `package.json` pins the exact pnpm version, which pnpm fetches for itself, and npm refuses to run in this repository.

Optionally, cache compiler outputs across checkouts with [Kache](https://github.com/kunobi-ninja/kache),
which pays off when every change gets its own worktree. Set it as Cargo's wrapper in your
user-level `~/.cargo/config.toml` (not the repository's, so CI stays plain):

```sh
cargo install kache --locked
printf '[build]\nrustc-wrapper = "%s"\n' "$HOME/.cargo/bin/kache" >> ~/.cargo/config.toml
```

A new worktree then restores whatever another checkout already compiled instead of
compiling it again, at the cost of about a second per rebuild within one tree.
Bypass it for one command with `KACHE_DISABLED=1`; `kache explain` says why a crate
missed the cache.

Open the URL Vite prints, normally `http://127.0.0.1:5173/sloppy-tanks/`. Vite
serves the release Wasm from `src/generated/engine/` (and the WebGL2 build from
`src/generated/engine-webgl/`); it does not compile Rust,
so run `pnpm run wasm` again after changing a crate.

Rendering uses WebGPU where the browser offers it (HTTPS or localhost, and a supporting browser and GPU). Elsewhere the page loads a separate WebGL2 build of the engine instead; WebGPU browsers never download it. Add `?webgl` to the URL to try the WebGL2 build on any browser. **Stats for nerds** shows the graphics API in use and rendering diagnostics.

To try a build on a phone or tablet, `pnpm run tunnel` builds the game and prints a temporary `https://….trycloudflare.com/sloppy-tanks/` link (needs `brew install cloudflared`; see [docs/cloudflare-tunnel.md](docs/cloudflare-tunnel.md)). Anyone with the link can open it while the command runs; restart it after changing the source.

## Play

Choose Skipper, Bruiser or Big Rig, then select a map and difficulty:

- **Team Battle:** six versus six, with respawns. First to 100 kills or the highest score after five minutes wins; a timed tie enters next-kill overtime.
- **Solo Assault:** survive ten minutes on one life against up to six enemies at once, with unlimited replacements.
- **Maps:** Pine Village, Harbor Havoc and Dusty Dig.
- **Difficulty:** Easy, Normal or Hard; fixed for the round and saved locally.

Your tank, single-player mode, map and difficulty are remembered in this browser.
The overhead/first-person view and overhead zoom are remembered too, and shared
between single player and multiplayer. Playing an extra level or an online team
battle keeps your preferred single-player mode for the next standard map.

Map links accept `?map=village`, `?map=harbor` or `?map=quarry`.

Opening the game with `?debug` adds two stress levels, marked **EXTRA**, to
the map dropdowns: **Stress Grid** (30 tanks among 75 destructibles) and
**Scrap Yard** (30 tanks in a compact yard whose cover rebuilds). In single player
they are an endless 15 v 15 team battle with your chosen tank; online they follow
the room's rules. Players are nearly invulnerable on both, and pickups are ten
times stronger. Rooms on these levels appear in the room list only on such a page;
their room links work for anyone. `?debug&map=superstress` links straight to
one.

| Control            | Action                                                   |
| ------------------ | -------------------------------------------------------- |
| WASD / arrows      | Steer toward a screen direction; opposite input reverses |
| Mouse              | Aim the turret                                           |
| Hold left click    | Fire                                                     |
| Right click        | Drop a mine                                              |
| 1–5 / ammo buttons | Select Standard, Spread, Rocket, Ricochet or Piercing    |
| Q / E / scroll     | Cycle stocked ammunition                                 |
| Shift + scroll     | Zoom                                                     |
| V / ◎ button       | Toggle the first-person view from the turret             |
| Escape / Pause     | Pause                                                    |
| ⚙ button           | Settings: touch controls, sound and battle speeds        |

In first person the camera sits on your turret: the mouse turns it (click the arena to capture the pointer; Esc frees the cursor and a second Esc pauses), WASD steers relative to where you look, the compass above the ammo strip shows which way the hull points, and pickups turn see-through. On touchscreens the drive stick's sideways push turns the view (forward and back drive toward it), and dragging the finger that fires fine-tunes the aim. While destroyed, you watch from above and the pointer stays captured for the respawn; press Esc to pick another tank.

Losing focus clears held input, while a hidden page pauses the round. Standard ammunition is unlimited; collect crates for special ammunition. Power-ups provide rapid fire, speed, shields, repairs and automatic laser defense. Enemy hull damage and kills earn Veteran, Elite and Heroic ranks; death resets rank progress. Timber and cargo break apart, drums explode, and destroyed cover opens routes. Allied tanks block shells without losing hull health or shields; rockets detonate on contact, with their existing self-damage rule. Bots hold fire when an ally blocks a firing lane, including spread pellets.

On iPads, car screens and other touchscreens, touch controls appear automatically: the same scheme as the phone edition, with the full game. Drag the stick in the bottom-left corner to drive (shorter drags move slower), and touch the arena to aim and fire: the turret turns toward your finger and fires until it lifts, and the finger can stay down and slide. Tap ✹ in the bottom-right corner to drop one mine, tap an ammo slot in the weapon strip between them to select it, and use − / + to zoom. The mine button shows its cooldown. Open Settings with the gear in the top right corner to choose **Touch controls: Auto / On / Off**; the preference is saved. Landscape gives the clearest view, and portrait is supported. The joystick UI and its styles load only when touch controls are enabled; desktop Auto and Off skip their downloads and hidden UI updates.

Phones (a touchscreen whose shorter side is under 600 px, or any device with `?phone`) get a limited edition: Battle Setup offers only the tank, the map and the two tabs, single-player rounds are Team Battles on Easy, and the arena shows nothing but the drive stick, a small scoreboard, the first-person ◎ and pause buttons with − / + zoom under them, and the mid-screen notices. Touch the arena to aim and fire: the turret turns toward your finger and fires until it lifts, and the finger can stay down and slide. In first person (◎, remembered on the phone), the drive stick turns instead of strafing: push it sideways to turn the view (the tank follows when driving) and forward or back to drive; touching the arena fires, and dragging that finger fine-tunes the aim. The gun sight and hull compass show. Phones start with a farther camera until you zoom (the zoom is remembered), draw no overhead aiming reticle, and neither zoom the page nor show the long-press magnifier. Multiplayer on a phone is one button with no room list: it joins the busiest open room (one whose battle is under way first), showing that room's map, or creates a room on the chosen map with bots filling the teams; a room link joins that room. In a room the menu keeps only the room code, what is happening, the results and the actions: room rules, team and tank changes need a larger screen. There is no aim stick, FIRE or mine button, ammo strip or Settings on a phone.

Team-only HUNTER Humvees make hit-and-run TOW attacks. They prefer isolated targets, plan an escape before firing, and withdraw behind cover (or open distance when no cover is available). After reloading and a short pause, they approach from a different position. They remain lightly armored and do not escort the player. They stop for 0.9 seconds to aim and stay exposed for 0.65 seconds after launch; a TOW deals 75 base damage. Most Humvee kills erupt in a fireball and tumble as a whole vehicle; roughly one in five instead leaves a quietly smoking wreck with a small hop.

Choose **END BATTLE** from the pause menu to finish early and see your current stats without declaring a winner. The game-over screen is a battle report: eliminations, busiest rolling minute, longest life, best killing spree, damage dealt, highest rank, average kills per minute, direct projectile hit rate, five-second multikills, low-hull kills, revenge, previous-life ordnance kills, mine kills, demolition, pickups, hull damage taken, and shield damage absorbed. Earned callouts celebrate feats such as ONE-TANK ARMY and DEAD BUT DANGEROUS. Survival time excludes pauses; direct hit rate excludes splash-only hits and counts spread pellets individually. Personal bests are saved in this browser separately for each mode, map, and difficulty. PLAY AGAIN immediately starts another round with your current settings; BATTLE SETUP returns to the menu.

## Development

```sh
pnpm run check          # the CI gate: formatting, Wasm and Vite builds, lint, types, clippy, server, all tests
pnpm run wasm           # release Wasm and its glue into src/generated/engine/
pnpm test               # page shell, import boundary and traffic-bot tests (needs pnpm run server:build)
pnpm run test:rust      # simulation, net, renderer and server tests (cargo test --workspace)
pnpm run validate       # ten seeded full matches and reset checks
pnpm run build          # production output in dist/ (builds the Wasm first)
pnpm run lint:fix       # safe ESLint fixes
pnpm run format         # Prettier for source, tests, scripts, styles and docs; cargo fmt for Rust
pnpm run check:browser  # browser checks against a running dev server (set SLOPPY_URL)
```

[AGENTS.md](AGENTS.md) is the development guide: code conventions and the simulation, determinism and rendering rules. [scripts/README.md](scripts/README.md) lists the browser checks and performance measurements, and [docs/rust-rewrite.md](docs/rust-rewrite.md) records how the engine moved from TypeScript to Rust.

| Crate / directory | Target        | Owns                                                                                     |
| ----------------- | ------------- | ---------------------------------------------------------------------------------------- |
| `crates/core`     | native + Wasm | Simulation, rules, bots, maps (`sim/`), meshes and models, multiplayer protocol (`net/`) |
| `crates/render`   | Wasm          | The WebGPU/WebGL2 renderer, WGSL shaders, presentation, effects and cameras              |
| `crates/web`      | Wasm          | The wasm-bindgen API: `Game` (single player) and `NetGame` (a room page)                 |
| `crates/server`   | native        | The multiplayer server: HTTP, WebSocket rooms, limits, monitor and dashboard             |
| `src/`            | browser       | The page shell: menus, HUD, input, touch controls, audio and the room page               |

| Area                           | Starting points                                                                                             |
| ------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| Startup and game loop          | `src/main.ts`, `src/game.ts`, `crates/web/src/game.rs`                                                      |
| Simulation and match lifecycle | `crates/core/src/sim/simulation.rs`, `crates/core/src/sim/match_state.rs`                                   |
| Driving, weapons and damage    | `crates/core/src/sim/{tank_driving,weapons,projectiles,damage}.rs`                                          |
| Bots and navigation            | `crates/core/src/sim/{ai,bot_strategy,bot_movement,navigation}.rs`                                          |
| Maps and scenery               | `crates/core/src/sim/maps.rs`, `crates/core/src/models/scenery.rs`, `crates/render/src/presentation/mod.rs` |
| Models and materials           | `crates/core/src/models/{tank_model,cover_model}.rs`, `crates/render/src/{model,material,shader}.rs`        |
| Balance and progression        | `crates/core/src/sim/{data,combat_rules,difficulty,veterancy}.rs`                                           |
| Multiplayer                    | `crates/core/src/net/`, `crates/web/src/net_game.rs`, `src/net/client.ts`, `crates/server/`                 |
| Controls, UI and sound         | `src/game/controls.ts`, `src/game/ui.ts`, `src/game/audio.ts`                                               |

The development build exposes `window.sloppy` for diagnostics; `?tweak` opens the development-only zoom panel. `?autoplay` assigns bot controls to the player slot.

Multiplayer is available on the [production site](https://sloppy-tanks.fridman.me/?multiplayer)
and the [dev site](https://sloppy-tanks-dev.fridman.me/?multiplayer); both use the same
game server. Battle Setup's tank and map choices sit above its two tabs,
**Single player** and **Multiplayer**, and stay put when you switch: the tank you
pick is the one you drive online, and the map is the one a new room plays.
The Multiplayer tab lists open rooms (it polls only while shown) below a **New room**
row that holds the new room's match length and Humans only choice. Your saved name
is prefilled; first-time players get a random bot name they can edit. **Auto**
picks the team with fewer human seats (including reconnect reservations); choosing
Blue or Red repaints the tank previews. The button where single player has **GO!**
reads **Create room** for the new room, which starts playing immediately, or
**Join room** once you choose an open room. Battle
Setup stays up with the room's progress while the arena loads and draws its first
frames, then the battle replaces it; a page that has already built a single-player
arena reloads into the room behind the same setup.
Friends can join later through the list or a link from **Copy invite link** in the
in-game menu: a room link opens Battle Setup with that room selected. Reloading a
room page, or a join that gives up, returns there too, with the reason shown in the
room list; joining again keeps a seat that is still reserved.
Listings show human player counts, map, bot mode, round time and score.

The online HUD shows power-up time remaining, highlights critically low hull,
and explains who destroyed you and which weapon caused it during respawn.

Online the battle keeps playing behind the in-game menu and Settings: while either
is open a bot drives your tank (in humans-only rooms it idles), and closing them
takes it back. The in-game menu shows the room's rules as text. Between rounds it leads with the
last round's result; everyone stays in the room, so **Play again** keeps the group
and its teams together, while **Battle Setup** leaves the room (closing it if you
were the last player). The host can unfold **Change rules**, and anyone can change
team or tank for the next battle. If the connection drops, a dialog
replaces the menu while the game reconnects on its own. If it gives up, the
dialog says why (lost connection, room closed, seat expired, seat opened in
another tab, or a game update) and offers what still works: try again, play
here, reload, or return to Battle Setup.

New rooms default to **Humans only (no bots)**. Empty seats stay empty, and
paused or disconnected players remain idle and vulnerable. Uncheck it when
creating, or pick a bot difficulty between rounds, to fill the six-versus-six
teams with bots; bots
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

Single-player downloads no multiplayer page code and opens no game-server connection.
Multiplayer loads its client and UI only on entry and does not run browser physics.
The [plan](docs/multiplayer-plan.md) records remaining playtest gates and the
[server guide](crates/server/README.md) describes local development and deployment.
Multiplayer runs on a stand-alone Rust server on a VPS, with rooms held in memory.

## Assets

Runtime textures, tank previews and sounds are checked in under `public/`. Development and production builds use these files directly. Regenerate them only when changing artwork or sound:

| Command                          | Output / requirements                                                                                                            |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `pnpm run generate:textures`     | Procedural pickup, house, barrel and armor artwork, followed by texture optimization; requires `cwebp`                           |
| `pnpm run optimize:textures`     | Optimized runtime textures from preserved sources; requires `cwebp`                                                              |
| `pnpm run generate:ammo`         | The four special-ammunition pictograms; requires `cwebp`                                                                         |
| `pnpm run generate:pickup-atlas` | The shared pickup atlas from the icons in `assets/texture-sources/pickups/`; also run by `generate:textures` and `generate:ammo` |
| `pnpm run generate:previews`     | Tank selection WebPs rendered from the actual models; requires Google Chrome                                                     |
| `pnpm run generate:audio`        | Thirteen MP3 effects; requires FFmpeg                                                                                            |
| `pnpm run generate:favicon`      | `public/favicon.svg`, drawn as isometric vector shapes                                                                           |
| `pnpm run generate:app-icons`    | The Home Screen icons in `public/icons/`: the favicon's tank on navy; requires Google Chrome                                     |

On macOS, install the offline encoders with `brew install webp ffmpeg`.

Source images and encoding guidance live in [assets/texture-sources](assets/texture-sources/README.md), including [harbor](assets/texture-sources/harbor/README.md), [quarry](assets/texture-sources/quarry/README.md) and [tree](assets/texture-sources/trees/README.md) notes. The optimizer uses 768px grass and sandstone, 512px dirt and concrete, and 512px armor wear and conifer foliage. Source artwork is outside the deployed directory.

For conifer artwork, run `node --import tsx scripts/generate-conifer-texture.ts` followed by `pnpm run optimize:textures`. Other tree patterns use `node --import tsx scripts/generate-tree-textures.ts`; the laser pictogram uses `node --import tsx scripts/generate-laser-pickup.ts`. The cottage and watchtower surfaces ([notes](assets/texture-sources/houses/README.md)) use `node --import tsx scripts/generate-house-textures.ts`, then `pnpm run optimize:textures`.

See [tank references](assets/tank-references.md) for model provenance and [water texture notes](public/textures/water/README.md) for its source and license.

## Deployment

A GitHub Pages workflow publishes on pushes to `main`, after the quality gate passes: the
pull-request check workflow's parallel jobs, which together run everything in
`pnpm run check` and build the site.
It builds `dist/` with `DEPLOY_BASE=/` for <https://sloppy-tanks.fridman.me/>, the
repository's Pages custom domain (a Namecheap CNAME to `vladf1.github.io`). The old
address <https://fridman.me/sloppy-tanks/> redirects there. Local `pnpm run build` and
the Vite dev server keep the default `/sloppy-tanks/` base.

The workflow sets `VITE_MULTIPLAYER_URL` to the VPS game server. It does not deploy that
server, but after the site deploys it points the server's container image tag
`:production` at the commit's image; the dev site uses a separate server (below). A
client only plays on a server built from the same shared sources: the `crates/core`
files and the crates they compile with, listed by `node scripts/content-version.mjs`,
and `pnpm run server:check-if-redeployment-required` says whether the live server
matches this checkout and warns when the live page cannot join it. Both report their
build at `/health`: release version (`version`), protocol, content version, commit,
local-change state and build time, and the server also its server build. The release
version is `MAJOR.MINOR.PATCH.BUILD`, such as `1.1.0.628`: `version` in package.json,
bumped by hand, and the Pages workflow's run number, which GitHub increments on every
deploy from main (local and pull request builds have only the first three parts).
Battle Setup shows it with the commit. The page's `/health` is a static
`health/index.html` that GitHub Pages serves after a redirect to `/health/`
(<https://sloppy-tanks.fridman.me/health>). The server's release, commit and time are
those of the first main build that shipped its server build, so they predate the
page's when later commits left the server unchanged. After merging changes to any of them, run
`pnpm run server:update` (or let auto-update pull it); until then, players on the
production site are asked to reload and cannot join. Server-only changes
(`crates/server`, its dependencies and build settings) keep clients compatible but
still need that update to take effect; client-only files (`crates/render`,
`crates/web`, the page shell) never do. See `crates/server/README.md` for the
updater, auto-update and the SSH fallback.

`DEPLOY_BASE` controls both Vite asset URLs and the engine download.

The build separates the interactive menu from the engine and audio. Battle Setup appears immediately with a progress strip while the engine, textures and hidden arena prepare. Pressing GO early changes the button to WAIT and confirms that the round will start automatically; there is no need to keep clicking. The arena's shaders and first frame are prepared before combat starts. GO reuses the prepared arena when its choices still match, or prepares the newly selected map while keeping the menu visible. Independent image downloads, device setup and GPU pipeline compilation overlap where possible. The engine (Rust simulation and WebGPU renderer) is one WebAssembly file, emitted as a separate hashed asset and requested from the page's head so it downloads while the menu loads. A second file holds the WebGL2 build, which the head script requests instead only when the browser gives no WebGPU adapter. Hosts should serve it as `application/wasm` with gzip or Brotli compression.

### Local dev deployment

`pnpm run deploy:dev` runs the normal checks, builds the current local checkout
(including uncommitted changes), and publishes to the separate `sloppy-tanks-dev`
Cloudflare Pages project. Install the Wrangler CLI and run `wrangler login` first
(or provide a Pages:Edit API token). The publisher sets the account, project and
`main` deployment branch itself, independent of the local Git branch. No Git
commit or push is required. `pnpm run build:dev` builds without publishing.
The dev build connects the **Multiplayer** tab to the dev multiplayer server,
`wss://sloppy-tanks-server.fridman.me:8443`: a second server process on the same VPS,
separate from production's, whose dashboard is
<https://sloppy-tanks-server.fridman.me:8443/dashboard>. `deploy:dev` redeploys that dev
server from the same checkout first (over SSH) so client and server versions match;
production multiplayer is never affected.

The dev game is at <https://sloppy-tanks-dev.fridman.me/> and the directory of
browser test pages is at <https://sloppy-tanks-dev.fridman.me/test-pages.html>.
The provider URL is <https://sloppy-tanks-dev.pages.dev/>. Its `/health`
reports the same build details as production's, including whether local changes were present.
The build goes to `dist-dev/` with the `/` base; production builds and deployments
stay separate. The Namecheap CNAME `sloppy-tanks-dev` points to
`sloppy-tanks-dev.pages.dev`; Cloudflare must associate a custom domain before its
CNAME changes, and all other DNS records stay as they are.
