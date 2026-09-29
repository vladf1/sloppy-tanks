# Multiplayer server

`sloppy-server` is one native Rust process that hosts every room in memory. It
serves `/health`, `/rooms`, `/stats`, `/dashboard` and the `/room/CODE` WebSocket
on Tokio and hyper, with its own RFC 6455 framing and permessage-deflate
(`src/websocket/`). Each room is a Tokio task (`room_task.rs`) running a
`RoomSession` (`session.rs`: socket limits, join timeout, the 50 ms timer) around
`sloppy_core::net::MatchHost`, the same simulation, rules and replication the
browser engine runs. On a Vultr VPS behind Caddy, the production site uses it at
`wss://sloppy-tanks-server.fridman.me` and the dev site uses a second, separate
process at `wss://sloppy-tanks-server.fridman.me:8443`.

```sh
pnpm run server:dev        # build for this machine and listen on 127.0.0.1:8787
pnpm run dev               # in another terminal; open ?multiplayer on the printed URL
```

The client uses `ws://127.0.0.1:8787` locally. `pnpm run server:check:players`
runs real player sockets on all maps against a running server. Use
`SLOPPY_SERVER_URL=wss://sloppy-tanks-server.fridman.me` with a listed
`SLOPPY_ORIGIN` such as `https://sloppy-tanks-dev.pages.dev` to test the VPS.
`pnpm run check:multiplayer` drives two Chrome contexts; `SLOPPY_SERVER` selects
a remote server for that browser check. For sustained traffic from other
regions, the traffic bots (a separate Cloudflare Worker) join open rooms on the
VPS; see `bots/README.md`.

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
`x86_64-unknown-linux-musl` binary the VPS runs; Rust's bundled `rust-lld` links
it (`.cargo/config.toml`), so macOS needs no cross toolchain.

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

| Variable          | Default              | Meaning                                                   |
| ----------------- | -------------------- | --------------------------------------------------------- |
| `HOST` / `PORT`   | `127.0.0.1` / `8787` | Listener; on the VPS only Caddy is public; `PORT=0` picks |
| `ALLOWED_ORIGINS` | local Vite origins   | Exact comma-separated origin allowlist                    |
| `MAX_ROOMS`       | `10`                 | Live rooms; new room codes beyond it get 503              |
| `TRUST_PROXY`     | `true` on loopback   | Rate-limit on the last `X-Forwarded-For` hop set by Caddy |

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
for `/rooms?extralevels` and shows the others only on a page opened with
`?extralevels`, or when following that room's link. A Scrap Yard room sends
roughly five times a standard room's snapshot bandwidth.

Room traffic uses permessage-deflate at zlib-rs level 2 (its fast strategy; level
1's quick strategy sent about a quarter more than the former Node server's zlib
level 1) with context takeover, which browsers negotiate natively; a client that
does not offer it gets plain frames. Monitor byte counts are measured before compression; the `wire` figures
count socket bytes after it.

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

## VPS deployment

`deploy/vps/` holds the Ubuntu setup:

- `provision.sh` installs Caddy (official repo), creates the `sloppy` service
  user, and allows only SSH, 80, 443 and 8443 through `ufw`. The server is a static
  binary and needs no runtime.
- The systemd unit (`/opt/sloppy-tanks/sloppy-server`, sandboxed, `MemoryMax`)
  and `/etc/sloppy-tanks.env` configure the service. The dev site's server is a
  second unit, `sloppy-tanks-dev` (`/opt/sloppy-tanks-dev/`, loopback port 8788,
  `/etc/sloppy-tanks-dev.env` with the dev origins and a smaller `MAX_ROOMS` and
  `MemoryMax`); `provision-dev.sh` installs only it, Caddy's config and the 8443 rule.
- The `Caddyfile` sets up automatic Let's Encrypt TLS for
  `sloppy-tanks-server.fridman.me`, an A record in the fridman.me DNS at
  Namecheap, and refuses `/stats`. Port 8443 of the same hostname, with the same
  certificate, forwards to the dev server. The deploy scripts reach the host by the same
  name.

The host and SSH user are in `scripts/vps-host.mjs`; deploys need key-based SSH
as root. The scripts trust a new host's key on first contact and refuse a
changed one. To move to another server or provider, provision it with
`SLOPPY_VPS_SSH=root@<new ip> pnpm run server:provision`, repoint the A record,
then run `ssh-keygen -R sloppy-tanks-server.fridman.me` so the scripts accept
the new host key. Caddy obtains the certificate once the record reaches the new
server.

```sh
pnpm run server:provision  # first time, or after editing deploy/vps/*; then deploys
pnpm run server:provision:dev  # only the dev server's unit, Caddy config and 8443 rule; then deploys it
pnpm run server:deploy     # pnpm run check, build the musl binary, upload, restart, wait for /health
```

`pnpm run deploy:dev` deploys the dev server first (`deploy-vps.mjs --dev`), waits
until its `/health` reports the checkout's content version, then uploads the dev
site; it never restarts production's server. It refuses
to upload a build without the multiplayer entry.

A restart or deploy ends every live room. The graceful `SIGTERM`/`SIGINT`
handler sends `room-reset` and close code 1012, so players see the room-ended
message. After a crash, clients reconnect by themselves and find a fresh lobby.
Stop load tests before deploying.

## Monitoring

```sh
pnpm run server:logs    # follow the journal: room lifecycle lines and minute summaries
pnpm run server:stats   # /stats JSON over SSH
pnpm run server:status  # systemctl status for the game server and Caddy
# the same for the dev server: node scripts/vps.mjs logs|stats|status --dev
# exit 1 when the live server needs a redeploy: clients are refused, or only
# server code changed; --dev checks the dev server, SLOPPY_SERVER_URL=ws://127.0.0.1:8787 a local one
pnpm run server:check-if-redeployment-required
```

The log has one line per event: a room is created, a player joins, disconnects
or leaves, the server closes a socket (with its close code and reason), or a
room ends (with the reason and room age). While any room is active, a summary is
logged each minute: rooms, players, sockets, traffic, CPU, memory, runtime lag,
and one line per room (map, phase, players, time, score, tick cost and debt,
traffic). An idle server logs one final summary and then stays quiet.

`GET /stats` returns the same figures as JSON for the last 10 seconds, summed
from the monitor's one-second readings, plus totals since start. It lists every
room code, including unlisted rooms, so it answers only direct loopback requests
without `X-Forwarded-For`, and Caddy also refuses the path. Traffic figures count
characters of JSON, which equals bytes for ASCII; `wire` figures are socket bytes
after compression, including WebSocket frame and handshake bytes.
`tickAvgMs`/`tickMaxMs` are the time spent in each 50 ms room timer callback; a
`debtMs` that keeps rising means the room is falling behind real time. Memory
comes from a counting global allocator; `gcMs` stays 0 (there is no garbage
collector) and remains only for the record's shape.

### Dashboard

`/dashboard` (https://sloppy-tanks-server.fridman.me/dashboard on the VPS, the dev
server's at https://sloppy-tanks-server.fridman.me:8443/dashboard, or
`http://127.0.0.1:8787/dashboard` locally) is a public, read-only page that
updates every second: CPU, the share of time the runtime's worker threads were
busy, runtime lag percentiles, memory, traffic on the wire and before
compression, players and rooms, messages per second by type in each direction,
the slowest room tick against its 50 ms budget, one row per room, recent room
events and host load, with charts of the last five minutes. Message types are
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
