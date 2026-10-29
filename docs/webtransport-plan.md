# WebTransport plan

Status: proposed, nothing implemented. WebSockets are the only transport (see the [multiplayer plan](multiplayer-plan.md) and the [server guide](../crates/server/README.md)). The facts about browsers, libraries and proxies below were checked in October 2026; check them again before starting each phase.

## Goal

Stop one lost packet from stalling everything behind it. TCP delivers in order, so a lost segment holds back every snapshot behind it until its resend arrives (head-of-line blocking). That lateness grows the adaptive playout buffer, which is everyone's view delay, and a stalled upload lets the server expire a player's held movement or fire (an input lapse). WebTransport runs over QUIC and adds unreliable datagrams: a lost snapshot or input then costs only itself.

Add WebTransport alongside WebSockets; do not replace them. WebSockets stay because:

- the traffic bots run as a Cloudflare Worker, which cannot open QUIC connections;
- some networks block UDP;
- iPads before iPadOS 26.4 and other older browsers have no WebTransport;
- the desktop and iPad apps speak WebSocket.

The work goes ahead only if Phase 0 shows stalls that matter on real networks.

## Decisions

| Decision           | Choice                                                                                                                            | Why                                                                                                                                                                                                       |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Transport          | WebTransport where the browser and network allow it, WebSocket otherwise                                                          | The WebSocket path keeps serving the traffic bots, UDP-blocked networks, older Safari and the apps.                                                                                                       |
| Where QUIC ends    | A separate bridge process on each machine (`wt-bridge`), which forwards each session to the game server over a loopback WebSocket | The server's TLS provider cannot do QUIC (below), and Caddy only proxies WebTransport to another WebTransport server. Loopback loses no packets, so the ordered TCP hop behind the bridge delays nothing. |
| Bridge stack       | Rust: `web-transport-quinn` on quinn with `ring` or `aws-lc-rs`, its own image, built only in CI                                  | It advertises the settings Safari negotiates. The C in its crypto stays out of the game server, whose SSH fallback deploy (`server:deploy`) keeps cross-compiling from macOS without a C toolchain.       |
| Ports              | The bridge owns UDP 443; Caddy keeps TCP 443 with HTTP/3 turned off                                                               | Restrictive networks that allow any UDP usually allow 443.                                                                                                                                                |
| Certificates       | The Let's Encrypt certificate Caddy already obtains for the machine's nip.io name; certificate hashes only for local servers      | Safari 26.5 and 26.6 reportedly fail handshakes pinned by certificate hash.                                                                                                                               |
| Snapshot datagrams | Each frame is the change from the last frame that client confirmed (protocol 8); WebSocket clients keep base = previous frame     | Today's frames must be applied in order (below). With base = previous frame the WebSocket bytes stay exactly as they are.                                                                                 |
| Rollout            | Chrome first behind a page parameter, then the default with WebSocket fallback                                                    | Safari's implementation is the newest and least proven; Roblox is launching its browser client on Chrome first for the same reason.                                                                       |

## Background

### Why moving the current protocol is not enough

- A WebTransport stream is reliable and ordered, like TCP: one lost packet holds up the stream behind it. Only datagrams avoid that.
- Snapshot frames are changes from the previous frame, and every moving field first carries on by its change in the previous frame (`crates/core/src/net/replication.rs`). A client must apply every frame in order from a baseline, and all clients of a round share one encoded frame. Sent as datagrams, one loss would break the client's copy.
- Inputs go out 20 times a second while held, and the server expires held movement and fire after 250 ms. They already tolerate loss if each datagram repeats the last few inputs and the server ignores a `seq` it has already applied.

So the upload side gains as soon as inputs travel as datagrams (Phase 2), and the download side, where most of the gain is, only once snapshots stop depending on order (Phase 3).

### QUIC in the Rust server

quinn needs a rustls crypto provider with QUIC support. The server's `rustls-rustcrypto` (0.0.2-alpha) sets `quic: None` on every cipher suite, and its QUIC header protection is an unfinished `todo!()`; `rustls-graviola` 0.4.0 also sets `quic: None`. `ring` and `aws-lc-rs` support QUIC but contain C, which the musl cross-compile from macOS cannot build without a C toolchain. The bridge avoids the choice: the game server keeps its pure-Rust TLS for the directory client and never handles QUIC.

### Proxies

- **Caddy:** WebTransport passthrough was merged on October 13, 2026 ([caddy#7669](https://github.com/caddyserver/caddy/pull/7669)) for v2.12.0; the latest release is v2.11.7 and the machines pin 2.11.4. It is experimental and off by default. Caddy ends the browser's session and opens a second WebTransport session to the upstream over HTTP/3, copying streams and datagrams across, so the upstream must still speak QUIC. It uses `webtransport-go` v0.13.0, which advertises `0x2b603742`, `0x2c7cf000` and `0x14e9cd29` but not `0xc671706a`; nobody has reported Safari through it.
- **nginx, HAProxy, Envoy:** HTTP/3, but no documented WebTransport proxying.
- No ready-made bridge from WebTransport to WebSocket or plain HTTP was found.

### Browsers

- Chrome, Edge and Firefox support WebTransport.
- Safari added it in 26.4 (March 2026), reportedly inside Network.framework, so Safari checks run on a Mac rather than in Linux CI. It negotiates only the RFC-track settings codepoint `0xc671706a` and cancels sessions with servers that advertise only the older draft pair ([hyperium/h3#347](https://github.com/hyperium/h3/issues/347), [#363](https://github.com/hyperium/h3/pull/363)). Handshakes pinned by `serverCertificateHashes` were reported failing on 26.5 and 26.6 and working on 27.0.1. It writes datagrams through `datagrams.createWritable()`, not `datagrams.writable`.
- `web-transport-quinn` sends both the RFC-track and the older codepoints; `wtransport` 0.7.2 sends `0xc671706a` and the older enable flag. Neither has a Safari test of ours yet.

### Roblox

At RDC 2026 (September) Roblox announced browser play by the end of 2026, Chrome first, other browsers later, without naming its networking. Unofficial reverse-engineering of its apps ([Boblox `FINDINGS.md`](https://github.com/metetv2021-cloud/Boblox), October 2026) describes:

- a new QUIC transport, RbxTransport, replacing RakNet;
- a browser path of plain QUIC with ALPN `h3`, routed by hostnames that encode the game server's address (`<token>-<ip>-<port>.<vip>.qdmux.roblox.com`), much like our nip.io names, while the apps put that address in a header in front of each packet;
- certificate hashes for the game server in the join reply;
- a control stream, each reliable channel on its own stream, and unreliable channels as datagrams that split large messages into fragments;
- BBRv2 congestion control, a 20 s idle timeout, a 10 s handshake timeout and a Retry on every connection;
- a browser path that was not reachable from the internet yet.

What this plan takes from it: separate streams for separate kinds of reliable traffic, a capability flag at join, Retry, and an idle timeout shorter than the seat grace with keep-alives. It does not take fragmented datagrams: losing any piece loses the whole message, so large frames go on a reliable stream instead.

## Architecture

```
browser ──QUIC, UDP 443──> wt-bridge ──WebSocket, 127.0.0.1:8787──> sloppy-server
browser ──TLS, TCP 443───> Caddy ─────WebSocket, 127.0.0.1:8787──> sloppy-server   (fallback, bots)
```

The game server keeps listening only on loopback, behind both. It learns about WebTransport only through the bridge's subprotocol.

### The bridge

- Accepts sessions at `https://<nip.io name>/room/CODE` on UDP 443. For each, it opens `ws://127.0.0.1:8787/room/CODE` with the subprotocol `sloppy-bridge.1` and the player's address and port in `X-Forwarded-For` and `X-Client-Port`, as Caddy does, so the server's per-IP limits apply unchanged.
- Sets `TCP_NODELAY` on its loopback socket (the server already sets it on the sockets it accepts), so small datagram messages never wait on Nagle's algorithm and delayed acknowledgements.
- Reads the loopback socket without pausing: datagrams go out at once, and reliable messages wait in a bounded queue per session, so a slow client's stream never holds up its datagrams. The server's 2 MB slow-reader limit cannot trip through the bridge, which drains loopback quickly, so the bridge enforces it and closes with 4002.
- Drops a datagram that is larger than the session's `maxDatagramSize` or that QUIC cannot send now; it never queues stale datagrams. quinn's datagram send buffer stays small.
- Sends a Retry when handshakes arrive faster than a set rate, limits handshakes per IP, keeps flow-control windows small, times handshakes out after 10 s, and keeps sessions alive with an idle timeout shorter than the 30 s seat grace.
- Serves the certificate Caddy obtained for the machine's name and reloads it after a renewal. Caddy's key files are readable only by root and both containers run unprivileged, so a host systemd path unit copies the certificate and key to `/var/lib/sloppy-tanks/tls`, readable by the bridge's user.
- On a server close, closes the session with the same code (1012 on a restart, 4002 for a slow reader).

### Between the bridge and the server

Every message on a `sloppy-bridge.1` socket starts with one envelope byte:

| Byte | Direction       | Meaning                                                               |
| ---- | --------------- | --------------------------------------------------------------------- |
| 0    | both            | Reliable text (JSON), on the session's control stream                 |
| 1    | both            | Reliable binary, on the session's control stream                      |
| 2    | both            | Datagram                                                              |
| 3    | bridge → server | Path figures once a second: QUIC round trip, packets sent and lost    |
| 4    | server → bridge | Reliable binary on a new one-way stream (baselines, oversized frames) |

The server accepts the subprotocol only on a loopback connection, as it already trusts `X-Forwarded-For` only from loopback. On the browser side, messages on a stream are framed as `[varint length][kind][payload]`, since QUIC streams carry bytes, not messages. Close codes map one to one between the WebSocket and the WebTransport session.

WebSocket compression covers one hop, and the bridge's loopback socket negotiates none, so a WebTransport client would receive uncompressed baselines. If measurement shows that matters, the server deflates large reliable messages itself, flagged in the frame, and the page inflates them with `DecompressionStream("deflate-raw")`. Datagrams stay uncompressed: each must be readable alone.

## Phases

### Phase 0: evidence (go or no-go)

- Collect `retransmitPercent` and input lapses from production's `/stats`, and late batches (Stats for nerds) from players on different networks.
- Extend `scripts/network/lossy-network.sh`, which drops only `proto tcp` from port 443, to UDP as well.
- Add a datagram mode to the development transport delay (`crates/core/src/net/transport_delay.rs`, a `loss` page parameter) that drops messages where `stall` holds them, so the page can compare the two before any server work.
- Record, at 1, 2 and 5 % loss: late batches per minute, the 99th percentile of time between snapshots, view delay, input lapses, bytes per second per client and room tick cost.
- Stop here if stalls are rare on the networks players use.

### Phase 1: one interface for both transports (no behaviour change)

- Core client (`crates/core/src/net/client.rs`): `ClientAction::OpenSocket` gains a `transport`; a datagram send action and a `socket_datagram` input join the socket calls. The fallback policy lives here, so the page and the apps share it: try WebTransport when the server offers it and the platform has it, fall back to WebSocket after 3 s or on failure, and remember the choice per server for the page session.
- Server: `RoomSocket::tcp()` (`crates/server/src/session.rs`) becomes a transport-neutral `path()`: `TCP_INFO` for WebSockets, the bridge's figures for WebTransport. `SocketHandle` gains a datagram send. The monitor and dashboard split sockets, traffic and loss by transport.
- Page: `src/net/transport.ts` adapts `WebSocket` and `WebTransport` to the same open, send, close and buffered-bytes shape. It feature-detects `createWritable()`, asks for `congestionControl: "low-latency"` and reads `maxDatagramSize`. Add it to the import allowlist and update `crates/web/tests/fixtures/page-json.json` for the new actions.
- Listings: a server whose machine runs the bridge says so in `/servers` (a setting in `server.env`), and the directory (`directory/listing.ts`) gives its `https://` address beside the `wss://` one, from the same nip.io name.

### Phase 2: the bridge with today's protocol, dev site only

- Build `wt-bridge` and its image in CI; add its Quadlet unit, the certificate copy, `ufw allow 443/udp` and Caddy's `servers { protocols h1 h2 }` to `deploy/server/`.
- The server accepts `sloppy-bridge.1` and carries today's messages on envelopes 0 and 1.
- Inputs as datagrams: each repeats the last three inputs, and the host applies only a `seq` newer than the last one it applied. An input with `actions` also goes on the control stream, so a one-shot action survives any loss.
- A `transport` page parameter forces either transport for testing.
- Expect a faster join (one round trip for QUIC against TCP, TLS and the WebSocket upgrade) and fewer input lapses, but no change to download stalls yet.

### Phase 3: snapshots as datagrams (protocol 8, new content version)

- One encoder for both transports: `StateStream` writes the change from a base frame to a target frame. Moving fields carry on by their change in the base frame times the gap in ticks, which keeps it exact. The host keeps the last 64 frames (about a second) with their motion. WebSocket clients use base = target − 1, which reproduces today's bytes and keeps one shared encoding; WebTransport clients use the newest frame they confirmed. Encodings are cached by base and target, so clients at the same point share bytes.
- Each datagram stands alone: round, target, base, tick, the seat's input `ack`, `hull` and `bodies`, the changes since the base, and the events and projectile path entries newer than the client's confirmed frame. The client skips events it already has by id; applying a path entry twice gives the same path.
- The client confirms the newest frame it applied in a small datagram at least 30 times a second, or with its inputs.
- The client mirror keeps a ring of the frames it applied. It applies a datagram only when its target is newer than what it shows and its base is still in the ring, skips older ones, and asks for a baseline when the base has aged out. Each application stays all or nothing, as today.
- A frame larger than the datagram limit (destruction bursts) goes on its own one-way stream (envelope 4). It is still a change from a confirmed base, so it applies whenever it arrives.
- Update the replication invariants in `docs/multiplayer-plan.md`: "one ordered delta stream" and "WebSockets neither reorder nor duplicate messages" become per transport.
- Measure the bytes lost to compression against the machines' monthly transfer (a standard room sends about 9 KB/s per client today), and the per-client encoding cost on the Scrap Yard.

### Phase 4: rollout and the apps

- WebTransport becomes the page default, with WebSocket fallback; Stats for nerds names the transport.
- Production machines get the bridge through provisioning after the dev site has run it for a while.
- The desktop and iPad apps keep WebSocket until the page shows the gain, then may add a quinn client in `crates/desktop/src/sockets.rs` against the same bridge (`ring` is fine natively).
- The traffic bots stay on WebSocket, which keeps that path exercised in production.

## Testing

- Core: `crates/core/tests/integration/net_replication.rs` compares the mirror with the authoritative state after every frame under seeded loss, reordering and duplication; host tests cover input datagram ordering and duplicate `seq`.
- Server: a `sloppy-bridge.1` socket through the independent WebSocket client the integration tests already use, including the loopback-only check and close codes.
- Bridge: end to end with a WebTransport client through the bridge to a running server.
- Browser: a Chromium check through a local bridge with a self-signed certificate passed as `serverCertificateHashes` (local only); Safari 26.x and 27 and an iPad by hand on the dev site, which has a real certificate.
- Phase 0's loss settings repeated after Phases 2 and 3, against the same builds over WebSocket.

## Risks

| Risk                                                            | Mitigation                                                                                               |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Safari negotiation or certificate handling changes again        | Chrome first; WebSocket fallback on timeout; test each Safari release on the dev site                    |
| The bridge holds datagrams behind reliable messages             | Read loopback continuously, separate queues, `TCP_NODELAY` on both ends; measure time through the bridge |
| A second public process (QUIC on UDP 443) is attacked           | Retry under load, handshake limits per IP, small windows, the server's limits behind it unchanged        |
| Bandwidth rises without compression                             | Measure in Phase 2; deflate large reliable messages in the server if needed                              |
| Per-client snapshot encoding costs too much CPU                 | Cache encodings by base and target; measure tick cost on the Scrap Yard before rollout                   |
| Certificate renewal leaves the bridge on an expired certificate | Reload on file change; add the bridge certificate's expiry to `server:status`                            |
| The gain is small on the networks players use                   | Phase 0 decides before any of the rest is built                                                          |

## Alternatives considered

- **QUIC inside the game server:** needs either a QUIC crypto module of our own on RustCrypto (`aes` and `aes-gcm` are already locked), checked against the RFC 9001 Appendix A vectors, or C crypto in the server; and it puts the server itself on the internet. Revisit if the bridge's hop costs too much.
- **Caddy's passthrough:** unreleased and experimental, its upstream must still speak QUIC, and its Safari support is untested.
- **The current protocol on one WebTransport stream:** the same head-of-line blocking as TCP.
- **Splitting large frames across datagrams:** losing any piece loses the frame; a reliable stream suits bursts better.

## Sources

- [caddyserver/caddy#7669: experimental WebTransport passthrough](https://github.com/caddyserver/caddy/pull/7669) and [#5421](https://github.com/caddyserver/caddy/issues/5421)
- [hyperium/h3#347: Safari WebTransport negotiation](https://github.com/hyperium/h3/issues/347) and [#363](https://github.com/hyperium/h3/pull/363)
- [W3C WebTransport](https://www.w3.org/TR/webtransport/) and [w3c/webtransport#525: reverse proxies](https://github.com/w3c/webtransport/issues/525)
- [MDN: WebTransport() constructor](https://developer.mozilla.org/en-US/docs/Web/API/WebTransport/WebTransport)
- [RustCrypto/rustls-rustcrypto](https://github.com/RustCrypto/rustls-rustcrypto), [ctz/graviola](https://github.com/ctz/graviola), [moq-dev/web-transport](https://github.com/moq-dev/web-transport), [BiagioFesta/wtransport](https://github.com/BiagioFesta/wtransport), [quic-go/webtransport-go](https://github.com/quic-go/webtransport-go)
- [Roblox: RDC 2026, The World Needs More Play](https://about.roblox.com/newsroom/2026/09/rdc-2026-the-world-needs-more-play) and [Boblox `FINDINGS.md`](https://github.com/metetv2021-cloud/Boblox) (unofficial)
