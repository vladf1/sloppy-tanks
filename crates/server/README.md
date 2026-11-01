# Multiplayer server

`sloppy-server` is one native Rust process that hosts every room in memory. It
serves `/health`, `/rooms`, `/servers`, `/stats`, `/dashboard` (also at `/`) and the `/room/CODE` WebSocket
on Tokio and hyper, with its own RFC 6455 framing and permessage-deflate
(`src/websocket/`). Each room is a Tokio task (`room_task.rs`) running a
`RoomSession` (`session.rs`: socket limits, join timeout, the room timer) around
`sloppy_core::net::MatchHost`, the same simulation, rules and replication the
browser engine runs. Production and the dev site each run it on their own Vultr
machine behind Caddy ([Deployment](#deployment)), at the nip.io name of its address;
the server directory (https://directory.sloppytanks.com) lists them.

```sh
bun run server:dev        # build for this machine and listen on 127.0.0.1:8787
bun run dev               # in another terminal; open ?multiplayer on the printed URL
```

A page on this machine lists this server's rooms (it answers as its own directory) and
plays on `ws://127.0.0.1:8787`, unless the build names a directory; `?directory=` (the
app's `--directory`) names another. `bun run server:check:players` runs real player sockets on all
maps against a running server; `SLOPPY_SERVER_URL=wss://<server>`
(a URL the directory lists) tests that one. For sustained traffic from other regions, the traffic bots (a
separate Cloudflare Worker) join open rooms on production's server; see `bots/README.md`.

Tests: `cargo test -p sloppy-server` covers settings, limits, the WebSocket codec
and deflate, sessions, the monitor and dashboard, and end-to-end rooms through
an independent client (`server`, `match_room`), and announcing to a fake server
directory (`directory`): modules of the one test binary `tests/integration/`. Reconnect, host
transfer, expiry and replication live in `crates/core/tests/integration/net_*.rs`.
`tests/traffic-bots.test.ts` in `bun run test` drives the built binary with the
TypeScript traffic bots.

## Build and settings

`bun run server:build` builds `target/server/sloppy-server` with the `server`
Cargo profile (optimized, unwinding, so one room's panic ends only that room).
`bun scripts/build-server.mjs --vps` cross-compiles the static
`x86_64-unknown-linux-musl` binary the server machines run; Rust's bundled `rust-lld` links
it (`.cargo/config.toml`), so macOS needs no cross toolchain. It targets x86-64-v3
(AVX2, BMI2) without FMA, which would change seeded physics results; read the comment
there before changing either.

`bun run server:build-docker-image` builds the same binary into a `linux/amd64`
Docker image, `sloppy-tanks-server:<server build>` and `:latest` (`Dockerfile`).
It cross-compiles with `--vps` as above in the normal Cargo target directory, so
compiled dependencies are reused locally and from CI's Cargo cache, and the image
only copies in the static binary, listening on `0.0.0.0:8787`. Try it with
`docker run --rm -p 8787:8787 sloppy-tanks-server`. The server machines run these
images (see [Deployment](#deployment)).

Both builds stamp the content version the browser engine also carries
(`scripts/content-version.mjs`): a hash of `crates/core` sources, their resolved
crate tree and `rust-toolchain.toml`. Mismatched clients are rejected with a
reload message, so after editing `crates/core`, rebuild the Wasm (`bun run
wasm`) and the server together. The build also stamps a `serverBuild`
fingerprint of the core and server sources, their dependencies and build
settings, which `/health` reports so deploys and the redeploy check notice
server-only changes that leave clients compatible. No client URL override is
accepted in production builds.

Settings come from the environment:

| Variable             | Default              | Meaning                                                                                      |
| -------------------- | -------------------- | -------------------------------------------------------------------------------------------- |
| `HOST` / `PORT`      | `127.0.0.1` / `8787` | Listener; on a machine only Caddy is public; `PORT=0` picks                                  |
| `MAX_ROOMS`          | `10`                 | Live rooms; new room codes beyond it get 503                                                 |
| `DIRECTORY_URL`      | none                 | Where to announce this server's rooms; unset or empty: none                                  |
| `SERVER_ENVIRONMENT` | none                 | The environment that lists this server, needed with `DIRECTORY_URL`; it titles the dashboard |

A malformed value stops startup instead of silently turning a limit off. A loopback
listener rate-limits by the last `X-Forwarded-For` hop, which Caddy sets.

## Rooms and limits

Rooms admit eight people, at most six per team. Explicitly leaving the last seat
disposes the match immediately; dropped connections retain a 30-second room/seat
grace. Idle lobbies/results expire after five minutes. After four hours a room
hosts no new battle: one under way may finish (up to its 99-minute length plus
overtime), then the room closes. Menu/hidden clients stop receiving snapshots
until they resume with a full baseline. Seat tokens stay in session storage,
never in shared room links.

A room on an extra level (the Stress Grid or the Scrap Yard) fills that level's
roster with bots (30 tanks, where players are nearly invulnerable, power-ups
last ten times longer and ammo crates hold ten times as much). Its host picks it
like any map. Plain `/rooms` lists only rooms on standard maps; Battle Setup asks
for `/rooms?debug` and shows the others only on a page opened with
`?debug`, or when following that room's link. A Scrap Yard room sends
roughly five times a standard room's snapshot bandwidth.

Room traffic uses permessage-deflate at zlib-rs level 2 (its fast strategy) with
context takeover, which browsers negotiate natively; a client that does not offer it
gets plain frames. Binary state messages (snapshot batches and
baselines) are deflated from 64 bytes, since consecutive batches repeat each other and
even small ones shrink with context takeover; text messages only from 1 KiB, below which
the block overhead outweighs the saving (`src/websocket/deflate.rs`). Monitor byte counts
are measured before compression; the `wire` figures count socket bytes after it.

`GET /rooms` returns public room metadata only, never names or seat tokens.
Rooms publish on lobby changes and every 20 seconds while active;
disconnected-empty rooms are removed immediately and stale entries expire after
45 seconds. The list holds at most 256 entries, evicting the least recently
refreshed. The client refreshes every five seconds while the visible browser
dialog is open, and stops on joining, creating or leaving the page. Ordinary
single player never loads or polls it.

`GET /servers` answers in the server directory's format with this one server, so a lone
local server is its own directory: `{"servers":[{"url":…,…}]}`, where the entry holds
`url`, the address the request came in on (`wss://` when Caddy received it over HTTPS,
otherwise `ws://`, with the request's host), `/health`'s fields, `startedAt` (when the
process started, ISO 8601 UTC), the latest `/stats` sample's `cpuPercent` (100 is one
core) and `rssMB` with the host's `cpus` and `hostMemoryMB` (absent for the first ten
seconds), and `rooms`, every room including extra levels. It shares `/rooms`'s CORS headers and rate limit.

With `DIRECTORY_URL` set to the directory's base address
(`https://directory.sloppytanks.com`) and `SERVER_ENVIRONMENT` to the environment
that lists the server (`production` or `dev`), the server PUTs the same listing, without
`url`, to `/servers?environment=` there as JSON. A room entering or leaving the list
(its first player joining, its last leaving, or the room ending) sends it at once.
Otherwise it compares the rooms every 2 seconds and sends the listing when they changed
since the directory last accepted it, or 15 seconds after that, so the directory knows the
server is still up; the load figures alone never trigger a send. Each request has 5
seconds. A failing directory is retried at
every check and logged only when announcing starts failing or works again. The
directory takes the server's address from the request's source IP, so the server
connects to it over IPv4 only. TLS is rustls on RustCrypto's pure-Rust cryptography (`rustls-rustcrypto`) with
Mozilla's root certificates compiled in (`webpki-roots`), which keeps the binary free of
C code and the image free of a CA store.

The server checks 8-character room codes, protocol/content versions, message sizes
(4096 bytes) and rates (65 messages/second/socket) before accepting authority. It
accepts any page origin: a room holds no cookies or credentials that another site's
page could borrow, and `/stats` lists room codes publicly anyway. Per process it allows 60
room connections/minute/IP, 120 room entries/minute overall, 120 listing
requests/minute/IP, 16 open sockets per room and 32 per IP, and `MAX_ROOMS` live
rooms; joining an existing room is never refused by the room cap. Each limiter
tracks at most 10,000 IPs and refuses new ones while all are inside their
minute. A socket whose unsent output passes about 2 MB is closed with 4002.

## Deployment

Production and the dev site each run game server machines, Ubuntu x64 VPSes at Vultr set
up identically. A machine runs the server as a container under Podman, behind Caddy in a
container of its own, both supervised by systemd through Quadlet units. CI builds
production's server images, which production pulls by hand or automatically; a developer
publishes the dev servers' images from a checkout, which the dev servers pull by
themselves; and an SSH deploy from a checkout covers any time CI or the registry cannot.

Every machine serves the nip.io name of its address (`45-63-56-58.nip.io` resolves to
45.63.56.58), for which Caddy obtains a Let's Encrypt certificate, so a machine needs no
DNS record of ours. There is no list of machines: the server directory lists the ones
that run, under their environment, and the scripts (`scripts/servers.mjs`) take them from
it, reaching each over SSH as root at its address. `--ip ADDRESS` names a machine the
directory does not list, such as one whose server is down or that is still booting.

### Images and tags

The `server` job in `.github/workflows/check.yml` builds the binary once, runs the
page-shell tests against it, then builds the image and pushes it to
`ghcr.io/vladf1/sloppy-tanks-server` (a public package) as:

| Tag             | Moved by                                    | Meaning                                       |
| --------------- | ------------------------------------------- | --------------------------------------------- |
| `<serverBuild>` | main's first build of it                    | One image per server build (`/health`'s hash) |
| `pr-<number>`   | each pull request push                      | That pull request's latest server             |
| `main`, `sha-…` | each push to `main`                         | Main's latest server, and per commit          |
| `production`    | the deploy workflow, after the site deploys | What production should run                    |

A commit that leaves the server build alone (page, renderer or docs changes) only
retags the existing image. Main's builds pass the deploy workflow's run number, which
completes the release version (`1.1.0.628`, `scripts/release-version.mjs`) the image
records as a label and as `SLOPPY_RELEASE` for `/health` and the dashboard. When main
first ships a server build a pull request already built, it restamps that image's
release, commit and time over the same binary layer; later builds keep them. Pull requests never move `production`, and nothing in CI
connects to the machines. Fork pull requests build the image without pushing.

The dev servers follow the same package's `dev` tag. `server:publish --dev`, which
`deploy:dev` runs, points it at the package's image of the checkout's server build when
there is one (CI's `<serverBuild>`, or an earlier publish's), and otherwise pushes the
checkout's build as `dev-<serverBuild>` and `dev`. A dev build never takes the bare
`<serverBuild>` tag: CI only retags an image it finds there, so a developer's build,
possibly from uncommitted files and their own toolchain, could reach `production`.

### On a machine

`deploy/server/` holds a machine's setup, which reaches it only when the machine is
created ([Machines](#machines)):

- `bootstrap.sh` is the whole setup, run once by the startup script at first boot. It
  fetches the other files below from the repository's `main`, writes the machine's
  nip.io name into the Caddyfile, installs Podman from Ubuntu's archive, the units and
  the updater, turns off SSH password login, allows only SSH, 80 and 443 through `ufw`
  and starts the image the machine follows.
- `sloppy-tanks.container` is the server's Quadlet unit in
  `/etc/containers/systemd/`, from which systemd generates `sloppy-tanks.service`.
  It runs the image on loopback port 8787 with a read-only root, no capabilities, no new privileges and a 700 MB memory cap without
  swap. Output goes to the unit's journal. It runs the image ID pinned in
  `/var/lib/sloppy-tanks/image`, which the updater copies into the drop-in
  `sloppy-tanks.container.d/image.conf`, so a crash or reboot restarts exactly what
  was running and never needs the registry. Podman has no daemon: besides the
  containers, only a small `conmon` process per container stays running.
- `caddy.container` runs Caddy's official image, pinned by version and digest. Both
  containers use host networking, so the server keeps its loopback listener behind
  Caddy, Caddy sees players' real addresses, and Podman publishes no ports around
  `ufw`. Caddy's config is `/etc/sloppy-tanks/caddy/Caddyfile`; its certificates
  stay in `/var/lib/caddy/` across restarts. To upgrade Caddy, change the image in
  `caddy.container` and create the machines again.
- `sloppy-tanks-update` (installed in `/usr/local/bin`) is the only thing that
  changes the pin. It pins the new image, restarts the service and waits up to 60
  seconds for `/health` to report the image's content version and server build
  (its labels); otherwise it restores the previous image. It keeps the current and
  previous image (tagged `sloppy-tanks-pinned:current` and `:previous`) and prunes
  the rest.
- `sloppy-tanks-update.timer` is auto-update. It follows the tag of the machine's
  environment (`SERVER_ENVIRONMENT`): `:production` every two minutes, or `:dev` every
  10 seconds (a drop-in `bootstrap.sh` writes on dev). A check reads the tag's digest
  with two small requests (an anonymous token and the manifest's headers) and pulls
  only when it has moved; one that cannot reach the registry changes nothing, and the
  journal hears only when checks start failing or work again. `bootstrap.sh` turns it
  on; `server:auto-update off` turns it off until `on` or the machine is created again.

### Deploying

```sh
bun run server:update                # pull :production and switch now
bun run server:update --image pr-12  # pull another tag (or full image name) and pin it
bun run server:update --dev --image pr-12   # try a pull request's server on the dev server
bun run server:publish --dev         # build here, push it as :dev, wait for the dev servers
bun run server:rollback              # switch back to the previous image
bun run server:auto-update on|off    # follow the environment's tag by itself (default), or not
bun run server:auto-update resume    # lift a hold (below) so auto-update follows again
bun run server:deploy                # SSH fallback: build here, copy the image, switch
```

Each acts on production's machine; `--dev` picks the dev site's.

With auto-update on, as every machine is by default, production's timer checks
`:production` every two minutes. A new image waits until nobody holds a seat (the
`players` count in `/stats`), or at most four hours for a server-only change and ten
minutes after a content change, when the new site is already sending visitors a
reload message. Anything other than following
`:production` leaves a hold: an SSH deploy, a rollback or pulling another tag pins
that image, and auto-update skips its checks until `server:auto-update resume` or
`server:update`. An image that fails its health check is not tried again until the
tag moves or `server:update` pulls it. `server:status` shows the pinned and previous
image, any hold, pending update or failed image, and auto-update's latest checks.

Dev's machines check `:dev` every 10 seconds and switch as soon as it moves, without
looking at who is playing. A hold on dev (`server:deploy --dev`, `server:rollback --dev`
or `server:update --dev --image`) lasts until the next publish moves `:dev`.

`server:publish --dev` runs `server:deploy-check`, points `:dev` at the package's image
of the checkout's server build if it has one, or else builds the image here with Docker
and pushes it as `dev-<serverBuild>` and `:dev`, then waits until every dev server the
directory lists reports the checkout's build through Caddy: the next check, then the
restart. It pushes before it looks: with no dev server listed (none exists, or one is
still booting) it has nothing to wait for, and a booting machine starts the build by
itself. It needs no SSH, but Docker logged in to ghcr.io with a token that can
write packages, once:

```sh
gh auth refresh -h github.com -s write:packages
gh auth token | docker login ghcr.io --username "$(gh api user -q .login)" --password-stdin
```

GHCR takes only classic tokens, which `gh`'s is; a classic personal access token with
`write:packages` works too.

`server:deploy` builds the image here with Docker (`server:build-docker-image`),
streams it to the machine with `docker save | ssh podman load` and switches to it,
so it needs neither GitHub nor the registry. It first runs `server:deploy-check`
(rustfmt, clippy and tests for only `sloppy-core` and `sloppy-server`), and for
production it refuses a checkout that is not a clean `origin/main`; `--force`
overrides that. It finishes once the server reports the checkout's build through
Caddy.

`bun run deploy:dev` publishes the dev server first (`scripts/server.mjs publish
--dev`), which waits until the dev servers' `/health` reports the checkout's build, then
uploads the dev site; it never touches production's machine. It refuses to upload a
build without the multiplayer entry.

A restart or deploy ends every live room. Stopping the container sends `SIGTERM`
and allows 10 seconds before killing it; the graceful handler sends `room-reset`
and close code 1012, so players see the room-ended message. After a crash, clients reconnect by themselves and find a fresh
lobby. Stop load tests before deploying.

### Machines

To add a machine, deploy an Ubuntu x64 instance in Vultr with your SSH key and, as its
startup script (cloud-init user data), `deploy/server/cloud-init/production.yaml` or
`dev.yaml`, saved once in Vultr's Startup Scripts. At first boot the user data writes
`/etc/sloppy-tanks/server.env` (`SERVER_ENVIRONMENT`, `DIRECTORY_URL`) and runs
`deploy/server/bootstrap.sh` from the repository's `main`, which sets the machine up and
starts the image its environment follows (`:production`, or dev's latest publish on
`:dev`). The server appears in the directory under its environment within a few minutes,
once Caddy has its certificate, with no SSH from here; from then on `deploy:dev` and the
other `server:*` commands reach it. A dev machine created before anything was published
to `:dev` starts production's image instead, so it is up and listed, and switches to the
first publish within 10 seconds; that fallback is no hold, since it still follows `:dev`.

Machines are never set up again in place: a changed `deploy/server/` file reaches a
machine once it is on `main` and the machine is created again, by destroying it and
deploying a new one with the same startup script, or by Vultr's Reinstall, which keeps
the address. To retire a machine, destroy it: it leaves the directory 45 seconds after
its server stops. The scripts trust a new machine's host key on first contact and refuse
a changed one, so after a Reinstall, or a new machine at an old address, remove the old
key with `ssh-keygen -R <ip>`.

## Monitoring

```sh
bun run server:logs    # follow the journal: room lifecycle lines and minute summaries
bun run server:stats   # /stats JSON over SSH
bun run server:status  # systemctl status for the game server and Caddy, and its image state
# add --dev for the dev server
# exit 1 when the live server needs a redeploy: clients are refused, or only
# server code changed; --dev checks the dev server, SLOPPY_SERVER_URL=ws://127.0.0.1:8787 a local one
bun run server:check-if-redeployment-required
```

The log has one line per event: a room is created, a player joins, disconnects
or leaves, the server closes a socket (with its close code and reason), or a
room ends (with the reason and room age). On Linux, a line about a room socket
ending also gives its TCP round trip and how many of the data segments sent to
that player were retransmitted (`| rtt 85 ms, 6 of 800 segments resent`). While
any room is active, a summary is logged each minute: rooms, players, sockets,
traffic, player round trips, retransmits and input lapses, CPU, memory, runtime
lag, and one line per room (map, phase, players, time, score, tick cost and
debt, traffic, round trips, retransmits and input lapses). An idle server logs
one final summary and then stays quiet.

The network figures show how often players' connections stall. TCP delivers in
order, so one lost segment holds up every snapshot behind it until the resend
arrives (head-of-line blocking). The figures are the kernel's `TCP_INFO` (lowest
round trip, data segments sent and retransmitted) for the TCP connection to the
player, read on Linux only; other systems report zeros. Behind a proxy on the same
host, such as Caddy on the server machines, the server's own socket only reaches the proxy, so
when it trusts its proxy and a room socket's upgrade carries `X-Client-Port` (the
player's source port, beside `X-Forwarded-For`), the monitor finds the proxy's
socket to that address and port in the kernel's list of the host's TCP sockets
(netlink `sock_diag`, what `ss` reads) once a second. A connection without that
header reads its own socket about once a second while it writes and once more as
it closes. A proxy on another machine leaves the figures empty. The
lowest round trip is the path's latency: the kernel's smoothed estimate also
counts the browser's delayed acknowledgements, tens of milliseconds while the
traffic flows mostly towards the player. An
input lapse is a player's held movement or fire running out because their next
input arrived more than 250 ms late, a stalled upload or a frozen page, so the
tank stopped while they still held the controls. The room page's Stats for
nerds shows the client's side: late batches, snapshot batches that arrived over
150 ms after the previous one.

`GET /stats` returns the same figures as JSON for the last 10 seconds, summed
from the monitor's one-second readings, plus totals since start. It is public,
like the dashboard, and lists full room codes; `/rooms` lists them too, except
for rooms whose players are all reconnecting. Until the first sample, 10 s after
start, only a direct loopback request without `X-Forwarded-For` (the update
timer's) samples on demand; others get 503. Traffic figures count
message bytes before compression (JSON text and binary state); `wire` figures are socket bytes
after compression, including WebSocket frame and handshake bytes.
`tickAvgMs`/`tickMaxMs` are the time spent in each room timer callback, which
fires as each simulation tick falls due while a round runs (60 a second; about
every 17 ms between rounds); a `debtMs` that keeps rising means the room is
falling behind real time.
`rttP50Ms`/`rttMaxMs` are the median and highest of the room sockets' lowest TCP
round trips,
`retransmitPercent` the share of the window's data segments that were
retransmissions (in room rows: since each seated player connected), and
`inputLapses` the input lapses in the window (in room rows: this match so far);
`totals` keeps the segment and lapse counts since start. Memory comes from a
counting global allocator.

### Dashboard

`/dashboard` (on every server at its own address, linked from the directory's overview, or
`http://127.0.0.1:8787/dashboard` locally) is a public, read-only page, titled with
the server's environment (`Sloppy Tanks - dev`, or `unknown` without
`SERVER_ENVIRONMENT`), that updates every second: CPU, the share of time the runtime's worker threads were
busy, runtime lag percentiles, memory, traffic on the wire and before
compression, players and rooms, messages per second by type in each direction,
the slowest room tick against its 17 ms budget, player round trips and
retransmits, one row per room (with its players' round trips, retransmits and
input lapses), recent room events and host load, with charts of the last five
minutes. Message types are
read from the start of each message without parsing it, and names outside the
protocol count as `other`. `src/dashboard.html` is plain HTML and script
compiled into the binary; its charts load uPlot from jsDelivr, pinned by version
and subresource integrity. Without the library the page still shows everything
except the charts.

The page reads `/dashboard/stream`, a Server-Sent Events stream that starts with
the recent history and events and then sends each reading. A process admits 10
viewers and 30 stream opens per minute per IP; the page retries on its own
through restarts. A room code is enough to join a room, so the dashboard shows
only its first three characters; full codes stay in `/stats` and the journal.
Runtime lag is how late a 10 ms probe timer ran beyond its schedule.
