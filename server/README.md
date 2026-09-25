# Multiplayer server

Two hosts serve the same routes (`/health`, `/rooms` and the `/room/CODE` WebSocket) and share `room-session.ts`, which applies the socket
limits, join timeout and 50 ms timer around `MatchHost`. `MatchHost` owns each
room's simulation, seats and protocol.

- **Node server (`node/`)** is what the dev site uses, at `wss://45-63-56-58.sslip.io`
  on a Vultr VPS. Rooms and the room list live in one process's memory.
- **Cloudflare Worker (`worker.ts`)** routes `/room/ABCDEFGH` to one `PlayerRoom`
  Durable Object. It stays buildable and deployable by hand. The old `Room` class
  is retained only for the disabled M1 hosting experiment.

```sh
npm run server:node:dev   # or: npm run server:setup && npm run server:dev (Worker)
npm run dev
```

Open `?multiplayer` on the printed Vite URL. The client uses `ws://127.0.0.1:8787`
locally. In another terminal, `npm run server:check:players` runs real player
sockets on all maps; `npm run server:check:lifecycle` checks reconnect and expiry.
Use `SLOPPY_SERVER_URL=wss://45-63-56-58.sslip.io` with a listed
`SLOPPY_ORIGIN` such as `https://sloppy-tanks-dev.pages.dev` to test the VPS.
`npm run check:multiplayer` drives two Chrome contexts; `SLOPPY_SERVER` selects a
remote server for that browser check. The separately deployed traffic bots
still target the dev Worker; see `bots/README.md`.

## Node server on the VPS

`server/node/build.mjs` bundles `node/main.ts`, `ws`, three.js and the compat Rapier
package (which inlines its WASM) into one `server/dist-node/server.mjs`. It stamps the
same content version into the bundle as the Worker build, so the VPS needs only Node 24.
Settings come from the environment:

| Variable              | Default              | Meaning                                                   |
| --------------------- | -------------------- | --------------------------------------------------------- |
| `HOST` / `PORT`       | `127.0.0.1` / `8787` | Listener; on the VPS only Caddy is public                 |
| `ALLOWED_ORIGINS`     | local Vite origins   | Exact comma-separated origin allowlist                    |
| `MULTIPLAYER_ENABLED` | `true`               | `false` refuses rooms and listings                        |
| `TRUST_PROXY`         | `true` on loopback   | Rate-limit on the last `X-Forwarded-For` hop set by Caddy |

The in-process rate limits have the same budgets as the Worker bindings: 60 room
connections/minute/IP, 120 room entries/minute overall, and 120 listing
requests/minute/IP. A socket whose unsent output passes about 2 MB is closed with 4002.

`deploy/vps/` holds the Ubuntu setup:

- `provision.sh` installs Node 24 (NodeSource) and Caddy (official repo), creates
  the `sloppy` service user, and allows only SSH, 80 and 443 through `ufw`.
- The systemd unit and `/etc/sloppy-tanks.env` configure the service.
- The `Caddyfile` sets up automatic Let's Encrypt TLS for the sslip.io name.

The host and SSH user are in `scripts/vps-host.mjs`; deploys need key-based SSH as root.

```sh
npm run vps:provision   # first time, or after editing deploy/vps/*; then deploys
npm run vps:deploy      # npm run check, upload server.mjs, restart, wait for /health
```

A restart or deploy ends every live room. The graceful `SIGTERM` handler sends
`room-reset` and close code 1012, so players see the room-ended message.

### Monitoring

```sh
npm run vps:logs     # follow the journal: room lifecycle lines and minute summaries
npm run vps:stats    # /stats JSON over SSH
npm run vps:status   # systemctl status for the game server and Caddy
```

The log has one line per event: a room is created, a player joins, disconnects or
leaves, the server closes a socket (with its close code and reason), or a room ends
(with the reason and room age). While any room is active, a summary is logged each
minute: rooms, players, sockets, traffic, CPU, memory, event-loop delay, and one
line per room (map, phase, players, time, score, tick cost and debt, traffic). An
idle server logs one final summary and then stays quiet.

`GET /stats` returns the same figures as JSON, sampled every 10 seconds, plus totals
since start. It lists every room code, including unlisted rooms, so it answers only
direct loopback requests without `X-Forwarded-For`, and Caddy also refuses the path.
Traffic figures count UTF-16 characters of JSON, which equals bytes for ASCII.
`tickAvgMs`/`tickMaxMs` are the time spent in each 50 ms room timer callback; a
`debtMs` that keeps rising means the room is falling behind real time.

## Cloudflare Worker

The checked-in Worker config defaults both protocols to disabled.
`npm run deploy:dev` now deploys the VPS server (it waits until `/health` reports
the checkout's content version), then uploads the dev site. It refuses to upload a
build without the multiplayer entry. To deploy only the Worker:

```sh
npm run server:deploy -- --var MULTIPLAYER_ENABLED:true
```

`npm run server:deploy` without an override disables new connections. Deployments
may end existing in-memory matches; stop tests before deploying. No command
changes the account subscription or either production Pages site.

Both builds compute the same content hash from game/network sources and pinned
engine versions. After editing those sources, **restart Vite and rebuild the
server together**; mismatched clients are rejected with a reload message. No
client URL override is accepted in production builds.

Rooms admit eight people, at most six per team. Explicitly leaving the last seat
disposes the match immediately; dropped connections retain a 30-second room/seat
grace. Idle lobbies/results expire after five minutes; absolute lifetime is 30 minutes.
Menu/hidden clients stop receiving snapshots until they resume with a full
baseline. Seat tokens stay in session storage, never in shared room links.

`GET /rooms` reads a separate `RoomDirectory` Durable Object containing only
public room metadata, never names or seat tokens. Player rooms publish on lobby
changes and every 20 seconds while active; disconnected-empty rooms are removed
immediately and stale entries expire after 45 seconds. The directory persists
at most 256 entries, evicting the least recently refreshed entry at capacity.
The client refreshes every five seconds while the visible browser dialog is open,
and stops on joining, creating or leaving the page. Ordinary single-player never
loads or polls it. Listings have the same exact-origin check and a separate
120 requests/minute/IP limit. Directory reads and metadata writes add hosting
usage; they are not included in the earlier gameplay-only quota estimates.

The Worker checks exact allowed origins, 8-character room codes, protocol/content
versions, message sizes and rates before accepting authority. The edge rate
bindings allow 60 connection attempts/minute/IP and 120/minute/location before
Durable Object lookup. Cloudflare rate limits are approximate and local to an
edge location, **not a hard account-wide spend cap**. The account plan, quota
usage and peak isolate memory still need owner-visible dashboard verification.
`wrangler tail --config server/wrangler.jsonc --format json` records runtime
outcomes during manual load tests; keep reports under ignored artifacts.

## Retained M1 experiment

The retained M1 **bot-only, credential-protected experiment** uses a separate protocol. It runs the existing 12-tank Team Battle in workerd, projects explicit
JSON state, and tests timers, destruction bursts, reconnect grace, and slow clients.
No production game build connects to it. No subscription is changed by these tools.

Use Node 24 or newer and install with `npm ci`.

```sh
npm run server:setup
npm run server:dev
```

In another terminal:

```sh
npm run server:check
SLOPPY_HOST_SECONDS=10 SLOPPY_HOST_EXTENDED=1 npm run server:check
```

`server:setup` generates an ignored 256-bit test credential and writes
`server/.dev.vars` plus the harness credential files under
`artifacts/performance/multiplayer/`. It reuses an existing key. Keep those files
private; never put the credential in a room link. The Worker checks authorization
before creating a Durable Object. The experimental destination is
`sloppy-tanks-server-dev`, separate from both production Pages sites.

The checked-in deployment is disabled by default. When authorized to run the
deployed experiment, enable it explicitly and supply the ignored secret file:

```sh
npm run server:deploy -- --var EXPERIMENT_ENABLED:true --secrets-file "$PWD/artifacts/performance/multiplayer/worker-secrets.json"
SLOPPY_SERVER_URL=wss://YOUR-EXPERIMENT.workers.dev SLOPPY_HOST_SECONDS=900 SLOPPY_HOST_MAPS=village SLOPPY_HOST_EXTENDED=1 npm run server:check
```

`npm run server:deploy` without the enable override restores the disabled
configuration. Do not deploy during a load run: in-memory matches are disposable.
The load harness records full-state sizes, actual UTF-8 snapshot bytes, RTT, frame
gaps, tick debt, version information, failures, and raw samples. The extended run
includes eight clients, a quiet client, two rooms, grace/expiry, deliberate overload
and restart, malformed input, and a reader that stops acknowledging snapshots.
Rooms stop after 30 seconds without clients and have a 20-minute absolute limit.

`SLOPPY_HOST_CLIENTS` chooses 1–8 clients; `SLOPPY_HOST_MAPS` is a comma-separated
list of `village,harbor,quarry`. One-client runs send only liveness pings. The
experiment's input acknowledgements measure traffic consumption, **not** player
control: all tanks remain bot-driven. `full`, `collapse`, `stall`, and `restart`
are privileged lab messages and are not the final multiplayer protocol.

Each snapshot contains changed complete entity records, including appearance
metadata. This deliberately measures a conservative JSON workload. It is not the
final bandwidth budget, delta schema, or latency gate. Repeat measurements with
the final serializer; keep logs and comparisons in ignored artifacts, not here.

The Rapier loader depends on wasm-bindgen internals from 0.20.0. Keep both Rapier
packages pinned to the same version and review `build.mjs` on upgrades. Server
types, lint, and bundling are part of `npm run check`; long runs are manual.
