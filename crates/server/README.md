# Multiplayer server

`sloppy-server` is one native Rust process that hosts every room in memory. It
serves `/health`, `/rooms`, `/stats`, `/dashboard` (also at `/`) and the `/room/CODE` WebSocket
on Tokio and hyper, with its own RFC 6455 framing and permessage-deflate
(`src/websocket/`). Each room is a Tokio task (`room_task.rs`) running a
`RoomSession` (`session.rs`: socket limits, join timeout, the 50 ms timer) around
`sloppy_core::net::MatchHost`, the same simulation, rules and replication the
browser engine runs. Production and the dev site each run it on their own Vultr
machine behind Caddy ([Deployment](#deployment)); production's is at
`wss://sloppy-tanks-server.fridman.me`.

```sh
pnpm run server:dev        # build for this machine and listen on 127.0.0.1:8787
pnpm run dev               # in another terminal; open ?multiplayer on the printed URL
```

The client uses `ws://127.0.0.1:8787` locally. `pnpm run server:check:players`
runs real player sockets on all maps against a running server. Use
`SLOPPY_SERVER_URL=wss://sloppy-tanks-server.fridman.me` with a listed
`SLOPPY_ORIGIN` such as `https://sloppy-tanks-dev.pages.dev` to test production's.
`pnpm run check:multiplayer` drives two Chrome contexts; `SLOPPY_SERVER` selects
a remote server for that browser check. For sustained traffic from other
regions, the traffic bots (a separate Cloudflare Worker) join open rooms on
production's server; see `bots/README.md`.

Tests: `cargo test -p sloppy-server` covers settings, limits, the WebSocket codec
and deflate, sessions, the monitor and dashboard, and end-to-end rooms through
an independent client (`tests/server.rs`, `tests/match_room.rs`). Reconnect, host
transfer, expiry and replication live in `crates/core/tests/net_*.rs`.
`tests/traffic-bots.test.ts` in `pnpm test` drives the built binary with the
TypeScript traffic bots.

## Build and settings

`pnpm run server:build` builds `target/server/sloppy-server` with the `server`
Cargo profile (optimized, unwinding, so one room's panic ends only that room).
`node scripts/build-server.mjs --vps` cross-compiles the static
`x86_64-unknown-linux-musl` binary the server machines run; Rust's bundled `rust-lld` links
it (`.cargo/config.toml`), so macOS needs no cross toolchain. It targets x86-64-v3
(AVX2, BMI2) without FMA, which would change seeded physics results; read the comment
there before changing either.

`pnpm run server:build-docker-image` builds the same binary into a `linux/amd64`
Docker image, `sloppy-tanks-server:<server build>` and `:latest` (`Dockerfile`).
It cross-compiles with `--vps` as above in the normal Cargo target directory, so
compiled dependencies are reused locally and from CI's Cargo cache, and the image
only copies in the static binary, listening on `0.0.0.0:8787`. Try it with `docker run --rm -p 8787:8787
sloppy-tanks-server` and `ALLOWED_ORIGINS` as needed. The server machines run these
images (see [Deployment](#deployment)).

Both builds stamp the content version the browser engine also carries
(`scripts/content-version.mjs`): a hash of `crates/core` sources, their resolved
crate tree and `rust-toolchain.toml`. Mismatched clients are rejected with a
reload message, so after editing `crates/core`, rebuild the Wasm (`pnpm run
wasm`) and the server together. The build also stamps a `serverBuild`
fingerprint of the core and server sources, their dependencies and build
settings, which `/health` reports so deploys and the redeploy check notice
server-only changes that leave clients compatible. No client URL override is
accepted in production builds.

Settings come from the environment:

| Variable          | Default              | Meaning                                                     |
| ----------------- | -------------------- | ----------------------------------------------------------- |
| `HOST` / `PORT`   | `127.0.0.1` / `8787` | Listener; on a machine only Caddy is public; `PORT=0` picks |
| `ALLOWED_ORIGINS` | local Vite origins   | Exact comma-separated origin allowlist                      |
| `MAX_ROOMS`       | `10`                 | Live rooms; new room codes beyond it get 503                |
| `TRUST_PROXY`     | `true` on loopback   | Rate-limit on the last `X-Forwarded-For` hop set by Caddy   |

A malformed value stops startup instead of silently turning a limit off.

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

Room traffic uses permessage-deflate at zlib-rs level 2 (its fast strategy; level
1's quick strategy sent about a quarter more than the former Node server's zlib
level 1) with context takeover, which browsers negotiate natively; a client that
does not offer it gets plain frames. Binary state messages (snapshot batches and
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

The server checks exact allowed origins, 8-character room codes,
protocol/content versions, message sizes (4096 bytes) and rates (65
messages/second/socket) before accepting authority. Per process it allows 60
room connections/minute/IP, 120 room entries/minute overall, 120 listing
requests/minute/IP, 16 open sockets per room and 32 per IP, and `MAX_ROOMS` live
rooms; joining an existing room is never refused by the room cap. Each limiter
tracks at most 10,000 IPs and refuses new ones while all are inside their
minute. A socket whose unsent output passes about 2 MB is closed with 4002.

## Deployment

Production and the dev site each have their own game server machine, an Ubuntu 26.04
x64 VPS at Vultr. `deploy/servers.json` lists both (`scripts/servers.mjs` reads it):
each machine's public `ip`, an optional `hostname` and the server's environment
`settings`. A machine runs the server as a container under Podman, behind Caddy in a
container of its own, both supervised by systemd through Quadlet units. CI builds the
server images; production pulls them by hand or automatically, and an SSH deploy from
a checkout covers the dev server and any time CI or the registry cannot.

A machine needs no DNS record of ours to be usable: Caddy obtains a Let's Encrypt
certificate for its nip.io name (`45-63-56-58.nip.io` resolves to 45.63.56.58) as
well as for its hostname, if any. Players use the hostname when there is one
(production's `sloppy-tanks-server.fridman.me`, an A record at Namecheap) and the
nip.io name otherwise. The scripts reach a machine over SSH by its ip and check its
server through its nip.io name, which reaches that machine whatever the hostname's
DNS says. The Pages workflow and the traffic bots name production's address too;
`tests/servers.test.ts` keeps them equal to the list.

### Images and tags

The `server` job in `.github/workflows/check.yml` builds the binary once, runs the
page-shell tests against it, then builds the image and pushes it to
`ghcr.io/vladf1/sloppy-tanks-server` (a public package) as:

| Tag             | Moved by                                   | Meaning                                       |
| --------------- | ------------------------------------------ | --------------------------------------------- |
| `<serverBuild>` | main's first build of it                   | One image per server build (`/health`'s hash) |
| `pr-<number>`   | each pull request push                     | That pull request's latest server             |
| `main`, `sha-…` | each push to `main`                        | Main's latest server, and per commit          |
| `production`    | the Pages workflow, after the site deploys | What production should run                    |

A commit that leaves the server build alone (page, renderer or docs changes) only
retags the existing image. Main's builds pass the Pages workflow's run number, which
completes the release version (`1.1.0.628`, `scripts/release-version.mjs`) the image
records as a label and as `SLOPPY_RELEASE` for `/health` and the dashboard. When main
first ships a server build a pull request already built, it restamps that image's
release, commit and time over the same binary layer; later builds keep them. Pull requests never move `production`, and nothing in CI
connects to the machines. Fork pull requests build the image without pushing.

### On a machine

`deploy/server/` holds the setup, which `server:provision` uploads with the
machine's `server.env` (its settings) and its Caddyfile (its names):

- `provision.sh` installs Podman from Ubuntu's archive, the units and the updater,
  turns off SSH password login and allows only SSH, 80 and 443 through `ufw`. It is
  safe to run again: it validates the Caddyfile with the pinned Caddy before
  replacing the live one, reloads Caddy in place (restarting it only for a new Caddy
  image) and restarts the server only when its settings or unit changed.
- `sloppy-tanks.container` is the server's Quadlet unit in
  `/etc/containers/systemd/`, from which systemd generates `sloppy-tanks.service`.
  It runs the image on loopback port 8787 with `/etc/sloppy-tanks/server.env`, a
  read-only root, no capabilities, no new privileges and a 700 MB memory cap without
  swap. Output goes to the unit's journal. It runs the image ID pinned in
  `/var/lib/sloppy-tanks/image`, which the updater copies into the drop-in
  `sloppy-tanks.container.d/image.conf`, so a crash or reboot restarts exactly what
  was running and never needs the registry. Podman has no daemon: besides the
  containers, only a small `conmon` process per container stays running.
- `caddy.container` runs Caddy's official image, pinned by version and digest. Both
  containers use host networking, so the server keeps its loopback listener behind
  Caddy, Caddy sees players' real addresses, and Podman publishes no ports around
  `ufw`. Caddy's config is `/etc/sloppy-tanks/caddy/Caddyfile`; its certificates
  stay in `/var/lib/caddy/` across restarts and upgrades. To upgrade Caddy, change
  the image in `caddy.container` and provision again.
- `sloppy-tanks-update` (installed in `/usr/local/bin`) is the only thing that
  changes the pin. It pins the new image, restarts the service and waits up to 60
  seconds for `/health` to report the image's content version and server build
  (its labels); otherwise it restores the previous image. It keeps the current and
  previous image (tagged `sloppy-tanks-pinned:current` and `:previous`) and prunes
  the rest.
- `sloppy-tanks-update.timer` is auto-update, off until enabled. It follows
  `:production`, so it is for production's machine.

### Deploying

```sh
pnpm run server:update                # pull :production and switch now
pnpm run server:update --image pr-12  # pull another tag (or full image name) and pin it
pnpm run server:update --dev --image pr-12   # try a pull request's server on the dev server
pnpm run server:rollback              # switch back to the previous image
pnpm run server:auto-update on|off    # production follows :production by itself, or not
pnpm run server:auto-update resume    # lift a hold (below) so auto-update follows again
pnpm run server:deploy                # SSH fallback: build here, copy the image, switch
pnpm run server:provision             # set a machine up, or again after editing deploy/server/*
```

Each acts on production's machine; `--dev` picks the dev site's.

With auto-update on, the timer checks `:production` every two minutes. A new image
waits until nobody holds a seat (the `players` count in `/stats`), or at most four
hours for a server-only change and ten minutes after a content change, when the new
site is already sending visitors a reload message. Anything other than following
`:production` leaves a hold: an SSH deploy, a rollback or pulling another tag pins
that image, and auto-update skips its checks until `server:auto-update resume` or
`server:update`. `server:status` shows the pinned and previous image, any hold or
pending update and auto-update's latest checks. The dev server never auto-updates;
it changes only on `deploy:dev`, `server:deploy --dev` or `server:update --dev`.

`server:deploy` builds the image here with Docker (`server:build-docker-image`),
streams it to the machine with `docker save | ssh podman load` and switches to it,
so it needs neither GitHub nor the registry. It first runs `server:deploy-check`
(rustfmt, clippy and tests for only `sloppy-core` and `sloppy-server`), and for
production it refuses a checkout that is not a clean `origin/main`; `--force`
overrides that. It finishes once the server reports the checkout's build through
Caddy.

`pnpm run deploy:dev` deploys the dev server first (`scripts/server.mjs deploy
--dev`), waits until its `/health` reports the checkout's build, then uploads the dev
site; it never touches production's machine. It refuses to upload a build without
the multiplayer entry.

A restart or deploy ends every live room. Stopping the container sends `SIGTERM`
and allows 10 seconds before killing it; the graceful handler sends `room-reset`
and close code 1012, so players see the room-ended message. After a crash, clients reconnect by themselves and find a fresh
lobby. Stop load tests before deploying.

### Machines

To set up a machine, create an Ubuntu 26.04 x64 VPS with a public IPv4 address and
your SSH key for root, put its address in `deploy/servers.json` and run
`pnpm run server:provision` (`--dev` for the dev site's). It installs everything,
starts CI's `:production` image (on dev, `deploy:dev` then replaces it with the
checkout's build), and waits until the server answers through Caddy at its nip.io
name, which takes about a minute
while Caddy obtains the certificate. The scripts trust a new machine's host key on
first contact and refuse a changed one; after replacing a machine at the same
address, remove the old key with `ssh-keygen -R <ip>`.

A hostname needs only its A record pointing at the machine and its entry in the
list; provision again so Caddy serves it. To move production to another machine, put
its address in production's entry and provision it: players stay on the old machine
until the hostname's record points at the new one, while the scripts already check
the new one through its nip.io name. `SLOPPY_SERVERS` points the scripts at another list, such as
one of local test machines: Caddy gives names ending in `.local` its own local
certificates, and private addresses get no nip.io name.

## Monitoring

```sh
pnpm run server:logs    # follow the journal: room lifecycle lines and minute summaries
pnpm run server:stats   # /stats JSON over SSH
pnpm run server:status  # systemctl status for the game server and Caddy, and its image state
# add --dev for the dev server
# exit 1 when the live server needs a redeploy: clients are refused, or only
# server code changed; --dev checks the dev server, SLOPPY_SERVER_URL=ws://127.0.0.1:8787 a local one
pnpm run server:check-if-redeployment-required
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
`tickAvgMs`/`tickMaxMs` are the time spent in each 50 ms room timer callback; a
`debtMs` that keeps rising means the room is falling behind real time.
`rttP50Ms`/`rttMaxMs` are the median and highest of the room sockets' lowest TCP
round trips,
`retransmitPercent` the share of the window's data segments that were
retransmissions (in room rows: since each seated player connected), and
`inputLapses` the input lapses in the window (in room rows: this match so far);
`totals` keeps the segment and lapse counts since start. Memory comes from a
counting global allocator; `gcMs` stays 0 (there is no garbage collector) and
remains only for the record's shape.

### Dashboard

`/dashboard` (https://sloppy-tanks-server.fridman.me/dashboard for production, the
dev server's at its own address from `deploy/servers.json`, or
`http://127.0.0.1:8787/dashboard` locally) is a public, read-only page that
updates every second: CPU, the share of time the runtime's worker threads were
busy, runtime lag percentiles, memory, traffic on the wire and before
compression, players and rooms, messages per second by type in each direction,
the slowest room tick against its 50 ms budget, player round trips and
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
