# Multiplayer server

A single Node process hosts every room in memory. `server.ts` serves `/health`,
`/rooms` and the `/room/CODE` WebSocket. `room-session.ts` applies the socket
limits, join timeout and 50 ms timer around `MatchHost`, which owns each room's
simulation, seats and protocol. The dev site uses it at `wss://45-63-56-58.sslip.io`,
on a Vultr VPS behind Caddy.

```sh
npm run server:dev
npm run dev
```

Open `?multiplayer` on the printed Vite URL. The client uses `ws://127.0.0.1:8787`
locally. In another terminal, `npm run server:check:players` runs real player
sockets on all maps; `npm run server:check:lifecycle` checks reconnect and expiry.
Use `SLOPPY_SERVER_URL=wss://45-63-56-58.sslip.io` with a listed
`SLOPPY_ORIGIN` such as `https://sloppy-tanks-dev.pages.dev` to test the VPS.
`npm run check:multiplayer` drives two Chrome contexts; `SLOPPY_SERVER` selects a
remote server for that browser check. For sustained traffic from other regions,
the traffic bots (a separate Cloudflare Worker) join open rooms on the VPS; see
`bots/README.md`.

## Build and settings

`build.mjs` bundles `main.ts`, `ws`, three.js and the compat Rapier package (which
inlines its WASM) into one `server/dist/server.mjs`, so the VPS needs only Node 24.
It stamps the same content hash of game/network sources and pinned engine versions
that the client build uses. After editing those sources, **restart Vite and
rebuild the server together**; mismatched clients are rejected with a reload
message. No client URL override is accepted in production builds. Keep
`@dimforge/rapier3d` and `@dimforge/rapier3d-compat` pinned to the same version.

Settings come from the environment:

| Variable              | Default              | Meaning                                                   |
| --------------------- | -------------------- | --------------------------------------------------------- |
| `HOST` / `PORT`       | `127.0.0.1` / `8787` | Listener; on the VPS only Caddy is public                 |
| `ALLOWED_ORIGINS`     | local Vite origins   | Exact comma-separated origin allowlist                    |
| `MULTIPLAYER_ENABLED` | `true`               | `false` refuses rooms and listings                        |
| `TRUST_PROXY`         | `true` on loopback   | Rate-limit on the last `X-Forwarded-For` hop set by Caddy |

## Rooms and limits

Rooms admit eight people, at most six per team. Explicitly leaving the last seat
disposes the match immediately; dropped connections retain a 30-second room/seat
grace. Idle lobbies/results expire after five minutes; absolute lifetime is 30 minutes.
Menu/hidden clients stop receiving snapshots until they resume with a full
baseline. Seat tokens stay in session storage, never in shared room links.

`GET /rooms` returns public room metadata only, never names or seat tokens. Rooms
publish on lobby changes and every 20 seconds while active; disconnected-empty
rooms are removed immediately and stale entries expire after 45 seconds. The list
holds at most 256 entries, evicting the least recently refreshed. The client
refreshes every five seconds while the visible browser dialog is open, and stops
on joining, creating or leaving the page. Ordinary single-player never loads or
polls it.

The server checks exact allowed origins, 8-character room codes, protocol/content
versions, message sizes (4096 bytes) and rates (65 messages/second/socket) before
accepting authority. Per process it allows 60 room connections/minute/IP, 120 room
entries/minute overall, 120 listing requests/minute/IP, and 16 open sockets per
room. A socket whose unsent output passes about 2 MB is closed with 4002.

## VPS deployment

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

`npm run deploy:dev` also deploys the server first, waits until `/health` reports
the checkout's content version, then uploads the dev site. It refuses to upload a
build without the multiplayer entry.

A restart or deploy ends every live room. The graceful `SIGTERM` handler sends
`room-reset` and close code 1012, so players see the room-ended message. After a
crash, clients reconnect by themselves and find a fresh lobby. Stop load tests
before deploying.

## Monitoring

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
