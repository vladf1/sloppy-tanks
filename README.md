# Sloppy Tanks

A browser tank game with destructible cover, team battles and solo survival. The game is Rust compiled to WebAssembly: the simulation runs on Rapier, a custom renderer draws handwritten WGSL through WebGPU (`wgpu`), and the desktop app's egui shell, built for the page, draws the menus and HUD and plays the sound. Multiplayer rooms run the same Rust simulation in a native Rust server.

## Run

Use Node.js 24 or newer, [pnpm](https://pnpm.io/installation) 11 or newer, and Rust through [rustup](https://rustup.rs). On macOS:

```sh
brew install node pnpm rustup   # once per machine
rustup toolchain install        # in the checkout: the toolchain rust-toolchain.toml pins
cargo install wasm-bindgen-cli --version 0.2.129 --locked
pnpm install                    # once per checkout or worktree, and after dependency changes
pnpm run wasm                   # build the page's Wasm; rerun after Rust or WGSL edits
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

Open the URL Vite prints, normally `http://127.0.0.1:5173/sloppy-tanks/`. The page
(`index.html`) is the desktop app's egui shell (`crates/desktop`) built for wasm32: Vite
serves its release Wasm from `src/generated/shell/`; it does not compile Rust, so run
`pnpm run wasm` again after changing a crate. The page's query reads as the app's command
line: `?debug` is `--debug`.

Rendering needs WebGPU (HTTPS or localhost, and a supporting browser and GPU).

To try a build on a phone or tablet, `pnpm run tunnel` builds the game and prints a temporary `https://….trycloudflare.com/sloppy-tanks/` link (needs `brew install cloudflared`; see [docs/cloudflare-tunnel.md](docs/cloudflare-tunnel.md)). Anyone with the link can open it while the command runs; restart it after changing the source.

## Play

The game also comes as a desktop app for Apple silicon Macs and Windows: download it
from the [latest release](https://github.com/vladf1/sloppy-tanks/releases/latest). The
same app builds for iPad (see [iPad](#ipad)).

Choose Skipper, Bruiser or Big Rig, then select a map and difficulty:

- **Team Battle:** six versus six, with respawns. First to 100 kills or the highest score after five minutes wins; a timed tie enters next-kill overtime.
- **Solo Assault:** survive ten minutes on one life against up to six enemies at once, with unlimited replacements.
- **Maps:** Pine Village, Harbor Havoc and Dusty Dig.
- **Difficulty:** Easy, Normal or Hard; fixed for the round and saved locally.

Your tank, single-player mode, map and difficulty are remembered in this browser.
The overhead/first-person view and overhead zoom are remembered too, and shared
between single player and multiplayer. Playing an extra level or an online team
battle keeps your preferred single-player mode for the next standard map.

Opening the game with `?debug` adds two stress levels, marked **EXTRA**, to
the map dropdowns: **Stress Grid** (30 tanks among 75 destructibles) and
**Scrap Yard** (30 tanks in a compact yard whose cover rebuilds). In single player
they are an endless 15 v 15 team battle with your chosen tank; online they follow
the room's rules. Players are nearly invulnerable on both, and pickups are ten
times stronger. Rooms on these levels appear in the room list only on such a page.

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
| ⚙ button           | Settings: sound, graphics, battle speeds                 |

Settings' **Graphics: Auto / High / Medium / Low** is saved like the other choices. Auto keeps the frame rate smooth: desktops start with the full picture, while tablets and phones start on Medium at one rendered pixel per CSS pixel without multisampling. As the frames allow, Auto lowers or raises the 3D scene's resolution in small steps and the quality tier with it. High is the full picture; Medium drops ambient occlusion and fine ground and cloud detail; Low also drops bloom and halves the shadow map. A fixed tier draws at full resolution and never adapts, and High also keeps multisampling on a tablet after a reload. Stats for nerds (`?debug`, then N) shows the tier, the render scale and the GPU's frame time.

In first person the camera sits on your turret: the mouse turns it (click the arena to capture the pointer; Esc frees the cursor and a second Esc pauses), WASD steers relative to where you look, the compass above the ammo strip shows which way the hull points, and pickups turn see-through. On touchscreens the drive stick's sideways push turns the view (forward and back drive toward it), and dragging the finger that fires fine-tunes the aim. While destroyed, you watch from above and the pointer stays captured for the respawn; press Esc to pick another tank.

Losing focus clears held input, while a hidden page pauses the round. Standard ammunition is unlimited; collect crates for special ammunition. Power-ups provide rapid fire, speed, shields, repairs and automatic laser defense. Enemy hull damage and kills earn Veteran, Elite and Heroic ranks; death resets rank progress. Timber and cargo break apart, drums explode, and destroyed cover opens routes. Allied tanks block shells without losing hull health or shields; rockets detonate on contact, with their existing self-damage rule. Bots hold fire when an ally blocks a firing lane, including spread pellets.

On iPads, car screens and other touchscreens, touch controls appear automatically (in the [iPad app](#ipad) too): drag the stick in the bottom-left corner to drive (shorter drags move slower), and touch the arena to aim and fire: the turret turns toward your finger and fires until it lifts, and the finger can stay down and slide. Tap ✹ in the bottom-right corner to drop one mine, tap an ammo slot in the weapon strip between them to select it, and use − / + to zoom. The mine button shows its cooldown. In a page, Settings' **Touch controls: Auto / On / Off** is saved: Auto shows them where the browser reports a touchscreen and from the first touch on a touch laptop, and Off keeps the keyboard and mouse whatever touches the screen. The desktop app plays with a keyboard and mouse.

Phones (a touchscreen whose shorter side is under 600 px, or any device with `?phone`) get a limited edition: Battle Setup offers only the tank, the map, the two tabs and the battle format (Team Battle or Solo Assault), single-player bots are always Easy, and the arena shows nothing but the drive stick, a small scoreboard, − / + zoom, the first-person ◎ and pause keys along the top, and the mid-screen notices. Touch the arena to aim and fire. In first person the drive stick's sideways push turns the view, and the hull compass shows over a small hull plate. Phones start with a farther camera until you zoom (the zoom is remembered) and draw no overhead aiming reticle. Multiplayer on a phone is one button with no room list: it joins the busiest open room (one whose battle is under way first), showing that room's map, or creates a room on the chosen map with Easy bots filling the teams. In a room the menu keeps only the room, what is happening, the results and the actions: room rules, team and tank changes need a larger screen. There is no mine button or ammo strip on a phone, and Settings opens from the pause menu. In an iPhone's browser Battle Setup suggests adding the game to the Home Screen, which gives it the whole screen.

Team-only HUNTER Humvees make hit-and-run TOW attacks. They prefer isolated targets, plan an escape before firing, and withdraw behind cover (or open distance when no cover is available). After reloading and a short pause, they approach from a different position. They remain lightly armored and do not escort the player. They stop for 0.9 seconds to aim and stay exposed for 0.65 seconds after launch; a TOW deals 75 base damage. Most Humvee kills erupt in a fireball and tumble as a whole vehicle; roughly one in five instead leaves a quietly smoking wreck with a small hop.

Choose **END BATTLE** from the pause menu to finish early and see your current stats without declaring a winner. The game-over screen is a battle report: eliminations, busiest rolling minute, longest life, best killing spree, damage dealt, highest rank, average kills per minute, direct projectile hit rate, five-second multikills, low-hull kills, revenge, previous-life ordnance kills, mine kills, demolition, pickups, hull damage taken, and shield damage absorbed. Earned callouts celebrate feats such as ONE-TANK ARMY and DEAD BUT DANGEROUS. Survival time excludes pauses; direct hit rate excludes splash-only hits and counts spread pellets individually. Personal bests are saved in this browser separately for each mode, map, and difficulty. PLAY AGAIN immediately starts another round with your current settings; BATTLE SETUP returns to the menu.

## Development

```sh
pnpm run check          # the CI gate: formatting, Wasm and Vite builds, lint, types, clippy, server, all tests
pnpm run wasm           # the page: the egui shell's release Wasm and glue into src/generated/shell/
pnpm run wasm:engine    # the sloppy-web engine into src/generated/engine/ (fixtures, game preview, WireView)
pnpm run wasm:all       # both
pnpm test               # script, directory and traffic-bot tests (needs pnpm run server:build)
pnpm run test:rust      # simulation, net, renderer and server tests (cargo test --workspace, without the desktop app)
pnpm run validate       # ten seeded full matches and reset checks
pnpm run build          # production output in dist/ (builds the Wasm first)
pnpm run lint:fix       # safe ESLint fixes
pnpm run format         # Prettier for tests, scripts, tools and docs; cargo fmt for Rust
```

[AGENTS.md](AGENTS.md) is the development guide: code conventions and the simulation, determinism and rendering rules. [scripts/README.md](scripts/README.md) lists the browser fixtures and performance measurements. The engine was ported from the TypeScript game at `35afd91` (in Git history) and differs from it on purpose in Rapier 0.36's contact response, zlib-rs level 2 WebSocket compression, square cargo fittings on the three moored harbor ships and ownerless damage sent as `owner: 0`.

| Crate / directory | Target        | Owns                                                                                     |
| ----------------- | ------------- | ---------------------------------------------------------------------------------------- |
| `crates/core`     | native + Wasm | Simulation, rules, bots, maps (`sim/`), meshes and models, multiplayer protocol (`net/`) |
| `crates/render`   | Wasm          | The WebGPU/WebGL2 renderer, WGSL shaders, presentation, effects and cameras              |
| `crates/web`      | Wasm          | The wasm-bindgen API: `Game` (single player) and `NetGame` (a room page)                 |
| `crates/server`   | native        | The multiplayer server: HTTP, WebSocket rooms, limits, monitor and dashboard             |
| `crates/desktop`  | native + Wasm | The app and the page: a window on wgpu (Metal, Direct3D 12, WebGPU), egui menus, HUD     |

| Area                           | Starting points                                                                                                               |
| ------------------------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| Startup and game loop          | `crates/desktop/src/{main,app}.rs`, `crates/web/src/game.rs`                                                                  |
| Simulation and match lifecycle | `crates/core/src/sim/simulation.rs`, `crates/core/src/sim/match_state.rs`                                                     |
| Driving, weapons and damage    | `crates/core/src/sim/{tank_driving,weapons,projectiles,damage}.rs`                                                            |
| Bots and navigation            | `crates/core/src/sim/{ai,bot_strategy,bot_movement,navigation}.rs`                                                            |
| Maps and scenery               | `crates/core/src/sim/maps.rs`, `crates/core/src/models/scenery.rs`, `crates/render/src/presentation/{model_catalog,theme}.rs` |
| Models and materials           | `crates/core/src/models/{tank_model,cover_model}.rs`, `crates/render/src/{model,material,shader}.rs`                          |
| Balance and progression        | `crates/core/src/sim/{data,combat_rules,difficulty,veterancy}.rs`                                                             |
| Multiplayer                    | `crates/core/src/net/`, `crates/web/src/net_game.rs`, `crates/desktop/src/room.rs`, `crates/server/`                          |
| Controls, UI and sound         | `crates/desktop/src/{input,audio}.rs`, `crates/desktop/src/ui/`                                                               |

`?autoplay` assigns bot controls to the player slot for the first single-player round.

Multiplayer is available on the [production site](https://sloppy-tanks.fridman.me/)
and the [dev site](https://sloppy-tanks-dev.fridman.me/), each with its own
game server. Battle Setup's tank and map choices sit above its two tabs,
**Single player** and **Multiplayer**, and stay put when you switch: the tank you
pick is the one you drive online, and the map is the one a new room plays.
The Multiplayer tab lists open rooms (it polls only while shown) below a **New room**
row that holds the new room's match length and Humans only choice. Your saved name
is prefilled; first-time players get a random bot name they can edit. **Auto**
picks the team with fewer human seats (including reconnect reservations); choosing
Blue or Red repaints the tank previews. The button where single player has **GO!**
reads **Create room** for the new room, which starts playing immediately, or
**Join room** once you choose an open room. Friends can join later through the list
or by typing the room's code; joining again keeps a seat that is still reserved.
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

**Stats for nerds** is available during multiplayer battles on a page opened with
`?debug`: click the bottom-right link or press **N**. Network rows show RTT, received update count/rate, update
age, server tick and input sequence sent/acknowledged. A received update is one
full-state message or snapshot batch; an input acknowledgement confirms the
server processed an input sequence. Feel rows show how far behind your own tank
other tanks are drawn (view delay), how long a click takes to show its muzzle
flash, how many of the shots drawn at once the server fired too and how close
their shells were to the server's, how far the
server's impact was from a tank you hit as your screen drew it (the server judges
your shells where you saw the targets, up to 200 ms back), how far the shells that hit
you were drawn from your tank just before they struck (enemy shells close in on your
tank's present), and your tank's prediction corrections by cause. Render includes GPU geometries and textures.

The [plan](docs/multiplayer-plan.md) records remaining playtest gates and the
[server guide](crates/server/README.md) describes local development and deployment.

## Desktop builds

`crates/desktop` is the same game as a native app for macOS and Windows, no browser:
the Rust `Game` and `NetGame` the page drives, drawn with wgpu on Metal or Direct3D 12,
with menus and HUD in egui and the textures and sounds from
`public/` built into the executable.
The window opens at 1280×800 points and has a minimum of 1100×720. Its menus and HUD
follow the web edition's desktop layout; fullscreen and Quit are native controls.

```sh
pnpm run desktop          # the app for this machine
pnpm run desktop:mac      # Sloppy Tanks.app for Apple silicon, macOS 11 or newer
pnpm run desktop:windows  # sloppy-tanks.exe, on Windows with the MSVC build tools
```

Both land in the ignored `artifacts/desktop/`: the Mac app beside a zip of it and an
installer package (`.pkg`, which puts it in `/Applications`) for sharing, and the
Windows executable. Build through the script rather than `cargo build`: it stamps the
content version multiplayer joins send, which must match the servers' (the same one the
page carries), as well as the release and commit Battle Setup shows. It adds the Apple silicon
and Windows targets with rustup when they are missing and uses the `desktop` Cargo profile, which
optimizes everything for speed with fat LTO and one codegen unit. Pull-request artifacts use
ThinLTO and parallel code generation to compile faster; main's releases and local builds
keep the full release optimization. CI keeps separate caches for these compiler settings:
each PR's first app build seeds its ThinLTO cache, and later pushes reuse it. Release
builds use the fat-LTO cache from main. `cargo run -p sloppy-desktop` works for quick local
tries, but servers refuse its unstamped content version. The
[Desktop app workflow](.github/workflows/desktop.yml) builds both, and the iPad app for
the simulator, on pull requests that
touch the crates or assets and on main, and keeps them as run artifacts, along with Cargo's
compilation timing reports. macOS clippy and shell tests run alongside the app builds in
the `desktop-check` profile, without optimizing the GPU and window dependencies, while
retaining workspace line tables for panic backtraces. Linux CI leaves
the crate out (`--exclude sloppy-desktop`), since its window and audio crates need
system libraries there. Main's builds also replace the repository's
[latest release](https://github.com/vladf1/sloppy-tanks/releases/latest), under file
names that stay the same, so these links always fetch the newest:
[Mac installer](https://github.com/vladf1/sloppy-tanks/releases/latest/download/Sloppy-Tanks-macOS.pkg),
[Mac app (zip)](https://github.com/vladf1/sloppy-tanks/releases/latest/download/Sloppy-Tanks-macOS.zip)
and [Windows](https://github.com/vladf1/sloppy-tanks/releases/latest/download/Sloppy-Tanks-Windows.exe).
The release job publishes only a Mac app and package that carry a stapled notarization
ticket.

### iPad

The same crate builds as an iPad app (iPadOS 16 or newer, landscape, full screen), with
the web edition's tablet touch controls: the drive stick in the bottom-left corner, a
finger on the arena to aim and fire, and the mine button with − / + zoom in the
bottom-right corner. A hardware keyboard plays as on a Mac (arrows or WASD, Space, 1–5,
Q/E, V). There is no Quit or fullscreen, and single player pauses when the app leaves the
screen. It needs a Mac with Xcode:

```sh
pnpm run ipad                         # artifacts/ipad/: Sloppy Tanks.app and an .ipa for devices
pnpm run ipad -- --install            # also install and open it on the connected iPad
pnpm run ipad:simulator -- --install  # artifacts/ipad-simulator/, opened in an iPad simulator
```

A device build is signed with an Apple Development identity and the provisioning profile
Xcode keeps for the bundle identifier (the team's wildcard profile covers it), and
installs only on the devices that profile lists; `SLOPPY_IOS_PROFILE` and
`SLOPPY_IOS_IDENTITY` choose others, and `SLOPPY_IOS_DEVICE` the iPad to install on.
Simulator builds need no signing. Like the Mac app it carries the content version, so it
joins the same multiplayer servers.

Mac builds from main, and local ones on a Mac holding the Developer ID certificates and
a `notarytool` profile, are signed and notarized, so the `.pkg` and the zip open without
a warning. Pull request builds carry only an ad-hoc signature: the first time,
right-click the package or the app and choose **Open**, or, once macOS has refused it,
choose **Open Anyway** in System Settings → Privacy & Security. The Windows executable
is unsigned, so SmartScreen asks once: **More info**, then **Run anyway**. Start it with
`--debug` (for example `"Sloppy Tanks.app/Contents/MacOS/sloppy-tanks" --debug`) for
the extra levels a `?debug` page offers. Settings and personal bests live in
`~/Library/Application Support/Sloppy Tanks/` on macOS and `%APPDATA%\Sloppy Tanks\` on
Windows.

## Assets

Runtime textures, tank previews and sounds are checked in under `public/`. Development and production builds use these files directly. Regenerate them only when changing artwork or sound:

| Command                          | Output / requirements                                                                                                            |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `pnpm run generate:textures`     | Procedural pickup, house, barrel and armor artwork, followed by texture optimization; requires `cwebp`                           |
| `pnpm run optimize:textures`     | Optimized runtime textures from preserved sources; requires `cwebp`                                                              |
| `pnpm run generate:ammo`         | The four special-ammunition pictograms; requires `cwebp`                                                                         |
| `pnpm run generate:pickup-atlas` | The shared pickup atlas, painted procedurally by its generator script alone; also run by `generate:textures` and `generate:ammo` |
| `pnpm run generate:previews`     | Tank selection WebPs rendered from the actual models; requires Google Chrome                                                     |
| `pnpm run generate:audio`        | Thirteen MP3 effects; requires FFmpeg                                                                                            |
| `pnpm run generate:favicon`      | `public/favicon.svg`, drawn as isometric vector shapes                                                                           |
| `pnpm run generate:app-icons`    | The Home Screen icons in `public/icons/`: the favicon's tank on navy; requires Google Chrome                                     |

On macOS, install the offline encoders with `brew install webp ffmpeg`.

Source images and encoding guidance live in [assets/texture-sources](assets/texture-sources/README.md), including [harbor](assets/texture-sources/harbor/README.md), [quarry](assets/texture-sources/quarry/README.md) and [tree](assets/texture-sources/trees/README.md) notes. The optimizer uses 768px grass and sandstone, 512px dirt and concrete, and 512px armor wear and conifer foliage. Source artwork is outside the deployed directory.

For conifer artwork, run `node --import tsx scripts/generate-conifer-texture.ts` followed by `pnpm run optimize:textures`. Other tree patterns use `node --import tsx scripts/generate-tree-textures.ts`. Rebuild the tree atlas after either with `node --import tsx scripts/generate-tree-atlas.ts`. The cottage and watchtower surfaces ([notes](assets/texture-sources/houses/README.md)) use `node --import tsx scripts/generate-house-textures.ts`, then `pnpm run optimize:textures`.

See [tank references](assets/tank-references.md) for model provenance and [water texture notes](public/textures/water/README.md) for its source and license.

## Deployment

A GitHub Pages workflow publishes on pushes to `main`, after the quality gate passes: the
pull-request check workflow's parallel jobs, which together run everything in
`pnpm run check` and build the site.
It builds `dist/` with `DEPLOY_BASE=/` for <https://sloppy-tanks.fridman.me/>, the
repository's Pages custom domain (a DNS-only CNAME to `vladf1.github.io` in
fridman.me's Cloudflare zone). The old
address <https://fridman.me/sloppy-tanks/> redirects there. Local `pnpm run build` and
the Vite dev server keep the default `/sloppy-tanks/` base.

The page holds no server address, only the server directory's (production's, unless the
shell's build names another in `SLOPPY_DIRECTORY_URL`). The directory is a Cloudflare
Worker (`directory/`, published with `pnpm run directory:deploy`) that every game server
pushes its rooms to; the page lists every server's rooms from it and connects to the
server that hosts the chosen room, or for a new room the one with the fewest rooms. A
machine pushes its rooms to the directory under its environment (production or dev),
which its startup script (`deploy/server/cloud-init/`) writes as `DIRECTORY_URL` and
`SERVER_ENVIRONMENT`; the shell lists the environment its build names in
`SLOPPY_SERVER_ENVIRONMENT`: production's, and `dev` for the dev site (`build:dev`). The workflow does not deploy the server, but after the site deploys it
points the server's container image tag `:production` at the commit's image; the dev
site uses a separate server on its own machine (below). A
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
`crates/web`, `crates/desktop`) never do. See `crates/server/README.md` for the
updater, auto-update and the SSH fallback.

`DEPLOY_BASE` controls the Vite asset URLs. The page is one WebAssembly file (the shell,
with the engine, menus and sounds), emitted as a hashed asset, and the textures it fetches
from `textures/` beside the page. Hosts should serve it as `application/wasm` with gzip or
Brotli compression.

### Local dev deployment

`pnpm run deploy:dev` runs the normal checks, builds the current local checkout
(including uncommitted changes), and publishes it to the separate `sloppy-tanks-dev`
Cloudflare Worker. Run `pnpm exec wrangler login` first (or provide an API token that
can deploy Workers and their custom domains), and log Docker in to ghcr.io once with a
token that can write packages (`crates/server/README.md` shows how with `gh`). The
Worker's account, name and domain come from `deploy/dev-site/wrangler.jsonc`,
independent of the local Git branch. No Git commit or push is required.
`pnpm run build:dev` builds without publishing.
The dev build connects the **Multiplayer** tab to the dev servers, which list themselves in
the directory under the dev environment. They run on their own machines, each at its nip.io
name (`wss://1-2-3-4.nip.io`, dashboard at `/dashboard`). `deploy:dev` first pushes
the same checkout's server image as its `:dev` tag, which those servers check every 10
seconds and switch to by themselves, and waits for them, so client and server versions
match; production multiplayer is never affected.

The dev game is at <https://sloppy-tanks-dev.fridman.me/> and the directory of
browser test pages is at <https://sloppy-tanks-dev.fridman.me/test-pages.html>.
The provider URL is <https://sloppy-tanks-dev.vova145.workers.dev/>. Its `/health`
reports the same build details as production's, including whether local changes were present.
The build goes to `dist-dev/` with the `/` base; production builds and deployments
stay separate. The hostname is the Worker's custom domain: Wrangler keeps its DNS
record in fridman.me's Cloudflare zone, and all other records stay as they are.

The Worker serves `dist-dev/` as static assets with the build's `_headers` (immutable
`/assets/`, `noindex` everywhere). Only the Wasm binaries run its script,
`deploy/dev-site/worker.mjs`. Cloudflare compresses static assets as it serves them,
which gives a multi-megabyte binary a later first byte and a larger file than Brotli at
quality 10. So `deploy:dev` first writes Brotli-10 and gzip copies beside each `.wasm`
(`deploy/dev-site/precompress.mjs`, a few seconds), and the script sends the one the
client accepts, marked `no-transform`. Range requests, and clients that accept neither, get the
original file.
