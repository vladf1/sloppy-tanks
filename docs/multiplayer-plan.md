# Multiplayer v1 plan

Status: the player server and browser client are implemented and published on the dev site. On 2026-09-25 hosting moved from Cloudflare Durable Objects to a stand-alone Node server on a Vultr VPS (rooms in memory; see the [server guide](../server/README.md)), and the Worker and Durable Object code was removed. Milestone text below that mentions Durable Objects or workerd records how the work was done at the time. The remaining gate is human feedback from different networks.

## Goal

Friends play one Team Battle together over the internet by sharing a room link. A server runs the match; browsers send controls and draw what the server reports. Hosting stays free or close to it, single-player keeps working, and seeded single-player results do not change.

Single-player must pay minimal cost for multiplayer support. Load the multiplayer session, connection, protocol validators, mirror, interpolation, prediction physics (if needed), and lobby UI through dynamic imports only when the player selects multiplayer or opens a room link. Normal single-player creates no multiplayer DOM, sockets, timers, or listeners and requests no multiplayer assets. The small shared render-state boundary must avoid per-frame scene copies. Compare production single-player entry/chunk sizes and cold-load requests against a pre-change build; add an automated absence-of-multiplayer-assets check, analogous to inactive touch loading. Keep server code and its dependencies out of all browser bundles.

## Decisions

| Decision           | Choice                                                                                                                                                | Why                                                                                                                                                             |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Who runs the match | The server: one Node process on a VPS, every room in memory                                                                                           | `Simulation` already runs without rendering. No host-tab problems; players can't fake damage. Originally one Cloudflare Durable Object per room.                |
| Transport          | WebSockets, JSON messages with rounded numbers in v1                                                                                                  | Simple to inspect. Delivery is reliable and ordered, so the protocol adds no retransmission. Measure the actual JSON protocol before setting bandwidth budgets. |
| Server address     | `wss://45-63-56-58.sslip.io`                                                                                                                          | sslip.io resolves to the VPS IP, so Let's Encrypt works with no DNS change; fridman.me stays at Namecheap.                                                      |
| Game mode          | Team Battle; up to 8 players, at most 6 per team; bots fill the 12 seats                                                                              | Preserves the existing six-versus-six roster. Players choose a team with room and a player-legal tank.                                                          |
| Latency handling   | Remote entities interpolated behind an adaptive 70–250 ms playout buffer; local aim immediate; local hull extrapolated at most 100 ms with correction | No movement prediction initially. If that test fails, bring prediction into v1 before building the client.                                                      |
| Portability        | Room logic in plain TypeScript, separate from any transport                                                                                           | Made the move from Durable Objects to Node a small wrapper change. Replacing the custom replication protocol with Colyseus would still be a separate project.   |
| Cost               | A small VPS at a flat monthly price                                                                                                                   | No per-request or duration billing. The limits are one CPU, 1 GB of memory and the plan's monthly transfer; watch them with `npm run vps:stats`.                |

Not planned for v1: WebTransport, accounts or public matchmaking, co-op Solo Assault, the full per-player battle report (v1 shows a scoreboard), persisted live-match recovery, and production rollout. Own-tank movement prediction is deferred only if M1b demonstrates acceptable controls at the tested latencies.

Considered and set aside:

- **Player-hosted matches:** the host tab pauses when hidden, browsers throttle background tabs, and the match ends when the host leaves.
- **Peer lockstep or whole-match deterministic rollback:** would require reproducibility across peers that has not been established. Matching aggregate results on two V8 builds is insufficient. Server-authoritative local prediction and reconciliation remain possible without that guarantee.
- **Colyseus:** needs a Node host. Keep it as an alternative if owning room and replication code becomes too costly; it does not remove the need to design game-specific input and lifecycle rules.
- **WebTransport:** Cloudflare can't host it; it needs a VM with an open UDP port.

## Implementation status

The [real player server](../server/README.md) is a Node process that wraps a platform-independent `MatchHost` per room in `RoomSession`. The client enters through a dynamic import, with its own connection, mirror, timeline and lobby. Normal single-player requests no networking modules, opens no socket, and allocates no network scene copies. Multiplayer does not download or initialize client Rapier physics.

- **M2 implemented:** independently driven seats, life/ownership separation, per-world tuning, bot takeover, explicit viewer presentation/audio/HUD/touch integration and no multiplayer battle recorder. The fresh seeded single-player validation still matches the captured pre-refactor baseline apart from wall time.
- **M3/M3a implemented:** versioned room/round/control identities, session-only seat tokens, input leases and acknowledgements, field deltas, atomic full baselines, resync, literal names, late joins, departed-player score attribution, host transfer and round transitions. Deterministic tests cover all maps, destruction, short-lived projectile paths, coalesced death/respawn effects, quaternion interpolation, slow readers, overload, suspension and repeated rounds.
- **M4 implemented; hosting acceptance remains conditional:** `/room/CODE` routes to the real host on the dedicated Worker. Eight-player deployed checks on all maps and quiet/grace/reconnect/expiry tests pass. Earlier long runs encountered abnormal WebSocket closes without a recorded CPU-limit exception; those failures remain in the evidence. A missing server close-handshake response was subsequently fixed and is covered by an automatic-reconnect browser regression. The post-fix four-player 15-minute run completed three rounds without room or seat loss. One abnormal socket close recovered with the same room epoch and player identity in under a second; retain this incident rather than describing the run as disconnect-free. No CPU-limit exception was observed in sampled tail output. This establishes tested recovery and sustained play, not an account-wide capacity guarantee. Account plan, consumed quota and peak isolate memory remain unverified.
- **M5 implemented:** create/share/join, map/difficulty and team/chassis choices, start, late join, independent controls, menu and hidden-tab takeover, reconnect, results and another map use the real network mirror. Two Chrome contexts pass this flow against local workerd, the deployed Worker and the published dev site. Real multi-touch and automatic reconnect pass at 0/50/100/150 ms added RTT and ordered jitter. A process-restart browser check verifies a fresh room with the same round number and a different map. The existing 21-check browser suite passes, and the production loading audit verifies isolation.
- **M1b/M6 acceptance:** the original latency experiment is retained as comparison evidence. The real client now reconstructs projectiles that launch and hit between updates, so its two-browser checks and timeline tests supersede the lab's incomplete combat sampling. The chosen starting policy is 20 Hz input, remote interpolation behind an adaptive playout buffer (one batch plus measured arrival lateness, independent of RTT) and bounded local and remote extrapolation, with a latency warning above 120 ms. This is a playtest default, not a claim that prediction is unnecessary at every RTT. Human control-feel judgment and several rounds on different networks remain required before production rollout.

The wire sends one shared JSON batch per 50 ms callback. It includes a frame at any intermediate lifecycle change and at the final simulation tick, retaining exact death/respawn/destruction timing. Tick-stamped swept projectile segments cover shots that never survive to a snapshot. Full scene state is sufficient to render without re-running map initialization. Lobby phase plus control/full messages announce starts; final state plus the results lobby carry the scoreboard, avoiding redundant standalone start/result messages.

Production UI checks must verify applied styles and visible controls, not just successful room messages. The inline startup build imports game entries outside Vite's normal dynamic-import graph, so it explicitly loads the selected entry's extracted CSS before starting it. Multiplayer styles remain absent from single-player requests. Browser checks cover lobby/results at desktop sizes, including maximum-length unbroken names and hidden actions. Phone support and phone viewport checks are out of scope; existing tablet touch controls remain supported.

Rooms default to **10-minute rounds**. The creator chooses 1–20 minutes in the room browser; only the host can change the next round's length in the lobby or results. The server validates and applies the duration, and room listings show it. Existing first-to-100 and tied-score overtime rules remain in effect. Single-player's round timer is unchanged.

During a battle, a compact team-colored player list shows every reserved human seat and its live authoritative kill count. Disconnected seats are dimmed during reconnect grace. The existing bounded activity feed announces new arrivals and reconnections; initial rosters, repeated lobby updates and opening the menu do not replay join messages. The list reuses snapshots, so it adds no polling or network messages.

The host can select **Humans only (no bots)** in the lobby or between rounds. The create dialog defaults it on; the retained direct-link lobby defaults it off. In that mode only assigned players spawn; empty seats have no tank or collider. Paused, silent and disconnected seats use an idle driver instead of AI, retaining normal vulnerability and reconnect grace. Explicit departures and expired reservations remove the tank without a fake death or replacement bot. Late joins create a fresh tank identity. The server validates the setting and prevents changes during a match; player/team limits and single-player behavior are unchanged.

The room browser reads an in-memory room catalog. `GET /rooms`
returns public metadata only, validates exact origins and permits 120 list
requests/minute/IP. Active rooms publish on membership/settings changes and every
20 seconds; empty entries are removed, stale entries expire after 45 seconds,
and the directory is bounded at 256 rooms. List polling runs every five seconds
only while the visible dialog is open and stops before entering a match.
Create requests atomically claim an empty code and start the
selected map; joining a stale listing fails instead of silently creating a room.
The last explicit departure disposes the simulation immediately; unplanned
connection loss retains the established reconnect grace.

Multiplayer Stats for Nerds exposes received update count/rate and age, RTT,
server tick, input sent/acknowledged sequence numbers and render diagnostics. Idle input sequence numbers advance once per second rather than at the active 20 Hz rate.
Updates count full-state messages and snapshot batches, not individual entities
or local inputs. Local hull heading uses shortest-arc smoothing between packets,
including angle wrap and a reset on respawn; this changes presentation only.

Wire regression ceilings are 160 KB per full baseline and 128 KB per burst frame for the seeded map/destruction fixtures, with a provisional 512 KB/s per-client sustained budget for the manual player load check. These are JSON budget ceilings with headroom, not measured throughput guarantees or billing limits. Swept projectile segments remain verbose; if playtests show bandwidth pressure, compact that representation before increasing player count. Keep byte measurements and all outliers in ignored artifacts.

Rate limits run before a room is created or joined: 60 connection attempts/minute/IP and 120/minute overall. They reduce accidental room creation. Rooms also enforce 8 players, 6 per team, message/action limits, a 30-second empty-room grace, 5-minute idle lobby/results expiry and 30-minute absolute lifetime. Hidden/menu clients receive no snapshot backlog and resume from a fresh full baseline.

Run `npm run check`, `npm run check:browser`, `npm run check:multiplayer-loading`, `npm run check:multiplayer`, and the [player/lifecycle harnesses](../server/README.md). The fresh seeded validation comparison must stay equal apart from timing. Raw results, retained failures, screenshots and comparisons belong under ignored `artifacts/performance/multiplayer/`; `SLOPPY_BASELINE_BUILD` supplies the production asset comparison. Long hosted runs and measurements remain outside CI.

## Hosting assumptions

- **Capacity:** one VPS with one CPU and 1 GB of memory runs every room in one process. A four-player room measured about 3 ms of simulation per 50 ms tick and 8% CPU; the server process uses roughly 130–200 MB. Watch `tickAvgMs`, `debtMs`, memory and event-loop delay in `npm run vps:stats` as rooms and bot fill grow, and record real peaks rather than extrapolating.
- **Bandwidth:** each client receives about 55–110 KB/s of JSON snapshots, uncompressed. That counts against the VPS plan's monthly transfer; binary encoding or compression would cut it.
- **Placement:** every room runs where the VPS is, so friends far from it pay that distance in RTT. Record per-player RTT in M6.
- **Restarts:** rooms are not persisted. A deploy or restart ends every live match with a room-reset notice; a crash leaves clients to reconnect into a fresh lobby.

## Architecture

```
Browser (Pages site)                              VPS (Caddy → Node)
┌──────────────────────────────┐   wss   ┌──────────────────────────────────────┐
│ Controls → input, 20/s       │ ──────► │ /room/CODE → RoomSession "CODE"      │
│ Render-state mirror          │ ◄────── │   (one per room, all in memory)      │
│ ← snapshots, 20/s            │         │   MatchHost → Simulation at 60 Hz    │
│ Presentation, audio, UI      │         │   seats, inputs, snapshots, events   │
└──────────────────────────────┘         └──────────────────────────────────────┘
```

New modules:

| Path                                | Role                                                                                                                                                     |
| ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/net/protocol.ts`               | Message types and protocol version, shared by client and server                                                                                          |
| `src/game/render-state.ts`          | Read-only presentation data: poses, entity appearance, HUD state and viewer identity; no Rapier body methods or simulation mutation                      |
| `src/net/match-host.ts`             | Room logic without any transport: seats, host controls, applying inputs, building snapshots and full state. Runs in Node tests and on the server         |
| `src/net/multiplayer-simulation.ts` | Validates player assignments, creates the team roster and changes a reserved seat's driver                                                               |
| `src/net/player-controls.ts`        | Per-seat input validation, leases, ordered actions and control epochs; no socket or timer ownership                                                      |
| `src/net/replication.ts`            | Client: applies validated full state and deltas to plain entity data, then supplies render state                                                         |
| `src/net/interpolation.ts`          | Client: snapshot buffer, server-clock estimate, per-entity interpolation                                                                                 |
| `src/net/connection.ts`             | Client: WebSocket, reconnect with a seat token, ping, and the dev-only delay harness                                                                     |
| `server/`                           | Node HTTP/WebSocket server, room sessions, room catalog, rate limits, monitoring, esbuild bundle and `tsconfig.json`; `deploy/vps/` holds the host setup |

Local single-player adapts its `Simulation` to the same read-only presentation boundary. The network mirror does not construct or step a gameplay simulation, mutate physics bodies for interpolation, or cast stand-in objects to `RAPIER.RigidBody`. Keep menu actions and simulation-specific diagnostics outside that read-only contract. Reuse persistent entity views or bounded buffers to avoid introducing a full scene allocation on every frame.

## Protocol v1

JSON messages with a `type` field and explicit serializers and validators. Reject malformed, oversized, non-finite or out-of-range values and unsupported tank/ammo choices. Names are length-limited plain text and rendered as text, not HTML. Do not serialize simulation objects by spreading them.

WebSockets deliver messages reliably and in order within a connection, and a reconnect starts a new control epoch and baseline. The protocol therefore adds no retransmission, message ids or per-action acknowledgements.

Round wire numbers in the serializer; unrounded floats print up to 17 digits, and 80 moving fragments would otherwise dominate collapse bursts. Initial precision: positions, velocities and dimensions to 0.001, angles and quaternion components to 0.0001 (renormalize quaternions after decoding), timers and health to 0.01. Record the chosen precisions as named constants beside the serializer.

### Identity and message ordering

- `version` identifies the wire schema. `contentVersion` identifies compatible map, balance and presentation data. Compute it at build time from a hash of the `src/game` and `src/net` TypeScript sources and the pinned Rapier and Three.js versions, using the same script in the client and server builds, rather than maintaining it by hand. A visual-only change then also forces a reload, which is acceptable for v1. A mismatch prompts a reload before state is applied. The same seed alone does not establish compatibility.
- `roomEpoch` is a new opaque value whenever the in-memory room is recreated; `roundId` identifies a round within it. A socket belongs to one room instance, so `welcome`, `lobby`, `control`, `full` and `room-reset` carry the epoch, while client messages and snapshot batches name only the round. Old-room and old-round traffic is discarded.
- Tank `id` persists across respawns; `life` changes each time and when the slot is reassigned. Keep lifecycle generation distinct from scoreboard deaths so seat takeover does not fabricate a death. Shots and mines preserve `ownerLife`. A separate `controlEpoch` changes on death, respawn, seat reassignment, reconnect and control handoff. Inputs must match the current assignment.
- Each successful join binds one socket to one seat. Reconnecting with the seat token revokes the old socket's authority before the new socket can control the tank. Never trust a client-supplied tank id to establish ownership.

### Client → server

- `join { version, contentVersion, name, kind, team, token?, roomEpoch? }`. Tokens are unguessable seat credentials, scoped to this room and kept out of shared room links. On reconnect the server preserves the seat's existing team and kind. Lobby-only `choose { team, kind }` updates an existing player's choices subject to the same capacity and kind validation.
- `input { roundId, controlEpoch, seq, observedTick, moveX, moveZ, aim, fire?, actions? }`, up to 20/s while moving, firing, aiming or delivering an action. Unchanged neutral input refreshes once per second to retain the seat; releases use the active cadence and the server still expires held movement/fire after 250 ms. Aim comparisons ignore sub-centimetre/0.001-radian render noise. The quota estimate remains the active-play upper bound. M1b compares 20 and 30 Hz sampling. `seq` increases within the control epoch; `observedTick` is the latest server tick seen by the client, used to reject excessively delayed input. `actions` lists, in order, the one-shot actions since the previous message; each is `mine` or `ammo { weapon }`. Omitted `fire` and `actions` mean not firing and no actions. Movement is sent to 0.01, aim points to 1 mm and aim angles to 0.0001 radian.
- `aim` is `{ x, z }`, the pointer's ground point, or `{ angle }` for touch-stick aim. Today `game.ts` computes the angle from the hull position; a network client's displayed hull lags the server's by about one round trip, so sending that angle makes shots miss sideways by the hull's drift. The server converts a point to an angle from the tank's current authoritative position every tick. `VehicleCommand.aim` remains an angle.
- `suspend` releases control when hidden or entering the local menu; `resume` requests a fresh baseline and control epoch. Both identify the round. They do not pause the shared match.
- `resync` requests a replacement full baseline. `leave` releases the seat immediately.
- Host only: `settings { mapMode, difficulty }` and `start` in the lobby; `end` while playing. Settings and team/kind choices are locked during a round.
- `ping { t }` for RTT and connection liveness; it does not renew the input lease.

### Input lifetime and one-shot actions

Initial defaults below become named constants and use the server's clock; adjust them only with recorded latency-test evidence.

- Between input messages, `MatchHost` supplies the last continuous command to every 60 Hz tick. A seat with no valid command gets neutral movement/fire while keeping its last aim.
- After 250 ms without fresh valid input, clear movement/fire and queued actions; the tank idles in place. After 5 s of silence on an open socket, the bot drives. Explicit suspension or socket closure hands control to the bot immediately. Once a bot drives, fresh input does not take the tank back directly: the server issues a new `control` epoch and the client resumes with it. After silence on an intact socket this needs no new baseline. After `suspend`, `resume` also requests one.
- One-shot actions enter a bounded ordered queue per seat, at most eight. `MatchHost` applies at most one per tick and drops actions older than 250 ms after receipt. Applying an action is one attempt, not a promise of success through a cooldown or missing ammo. Its outcome is visible in state (a mine, the selected ammo, a notice), so actions need no ids or acknowledgements.
- Each client's snapshots report `ack`, the highest input `seq` consumed for its seat. Newer controls may coalesce older unconsumed controls; queued actions remain ordered and can apply on later ticks. This acknowledgement measures control consumption, not completion of every action or a full movement replay interval. Prediction still requires the separate timing contract in M1b. Input more than 500 ms behind the current server tick by `observedTick` is rejected, as are impossible future ticks. Test these limits in the M1b and M5 latency cases. The host calls each seat's command consumer once per simulation tick.
- Send absolute ammo choices on the wire. Resolve wheel/next/previous input locally against the latest inventory and pending selection, then accept the authoritative selection returned by the server. Superseding a relative increment must not lose an intended change.
- Death, respawn, round changes and control transfers clear old controls and queued actions. Reconnect clears the client's pending actions; do not replay a mine click after reconnect or into another tank life.

### Server → client

- `welcome { version, contentVersion, roomEpoch, playerId, token, hostId }` and `lobby { players, settings }`.
- A `lobby` message with a new `roundId` and `phase: "playing"` announces a round. A `control` message supplies `tankId`, `life` and the current `controlEpoch` on assignment, handoff and respawn. Input stays disabled until the matching full state or respawn state has arrived.
- `full { roomEpoch, roundId, seq, tick, state: { elapsed, match, map, entities }, eventCursor }` on start, late join, resume, reconnect and resync. Include every current tank, cover (including destroyed covers and collapse rubble), pickup, shot, mine and fragment, using server-issued ids. Include settled transforms as well as moving ones. Fragment metadata includes shape, dimensions, color, wreck part/team, creation time and cleanup state; cover metadata includes hp, alive, timber hits and geometry/appearance changes. Full state must be sufficient without replaying earlier destruction events or relying on identical simulation initialization. If measured full-state size warrants it, untouched authored cover may be referenced by layout index instead of repeating its geometry, since a matching `contentVersion` guarantees the same layout.
- `snapshot { roundId, ack, snapshots: [frame] }`, 20/s, contains ordered `{ seq, tick, elapsed, match?, updates?, removed?, events?, traces? }` frames from one stream per round. A batch normally has one frame, plus intermediate lifecycle frames when needed. All active clients share the same serialized batch. Serialize the shared body once per snapshot. Each client's message wraps it with that seat's input `ack`, added without re-serializing the body. `match` holds only changed match fields. `updates` maps entity kind to entity id to changed fields (all fields on creation, `null` deleting an optional field); `removed` maps entity kind to removed ids. Empty sections are omitted; omission means unchanged, never deleted. A `traces` segment covers a shell's straight flight within the frame and carries only its drawable fields. Tank updates include life, pose, velocity, heading, aim, health/shield, recoil, rank, timers, ammo and kills/deaths. Include all fields required by rendering and HUD calculations, and leave out authoritative bookkeeping the client never reads (shell ownership and damage, cover navigation footprints, the previous physics pose).
- Final authoritative state plus `lobby { phase: "results", scoreboard, ... }`, `pong { t, tick }`, and explicit `room-reset` / `error` messages. Heartbeats include the round so in-flight old-round pings cannot be mistaken for future ticks. A lost in-memory match returns players to the lobby with a reason, rather than pretending to resume it.

### Replication invariants

- Each round has one ordered delta stream. A full baseline captures state and the event cursor at the tick of snapshot `seq` N; the client then applies N+1, N+2, … in order. Apply a full baseline atomically, clear superseded interpolation/effect queues, and replace all previous mirror entities. If a snapshot's `seq` is not the next expected one, stop applying deltas and request resync. Bound buffering during resync and restart it if the bound is exceeded.
- WebSockets neither reorder nor duplicate messages within one connection, so the harness exercises coalesced rendering and reconnect boundaries instead. Apply every lifecycle delta even if rendering skips intermediate poses; dropping an entire snapshot can discard a removal or a final resting position.
- A moving fragment or cover sends its final pose when it sleeps and a fresh update when it wakes. Fragment cleanup transitions and final removals are explicit, including removal caused by capacity limits. A destroyed tree remains a cover record so its stump can render; removing a fragment is a different operation.
- A new tank life or teleport clears that entity's interpolation history. Never interpolate from a death position to a respawn. Seat takeover updates player metadata without transferring old-life actions or XP attribution.
- Events carry a monotonically increasing id and server tick, plus entity/life identifiers where relevant. Apply state independently of transient effects. Schedule spatial effects against the same display timeline as their associated remote entities, and play each once. Full state establishes a cursor; reconnect does not replay old explosions or sounds. Handle local HUD feedback separately where immediate feedback is appropriate.
- Applying a lifecycle change to the authoritative mirror does not immediately apply it to the delayed display. Retain bounded render history until the display clock reaches a remote entity's creation, death, destruction, or removal tick, then change its visible state alongside its effects. Test a death followed by its explosion and a projectile born and destroyed between successive snapshots; snapshot-boundary equality alone cannot verify those visuals. Carry sufficient tick-stamped launch/impact data for such short-lived shots, without simulating combat on the client.
- JSON encode/decode tests must preserve meaning, not merely avoid exceptions. Indestructible cover and rubble currently use `Infinity`: encode indestructibility explicitly and reconstruct the intended local representation. Assert finite wire numbers, schema validity, and decoded state equivalent within the wire rounding, with no Rapier objects or handles leaking into messages.

### Room and seat lifetime

- A disconnected seat is reserved for 30 s while its tank is bot-driven. Reconnect reclaims it with a fresh control epoch. After expiry, release the seat and token; a subsequent join is a new player assignment. An explicit leave releases it immediately.
- If the last socket closes, keep the room and its timer alive for a bounded 30 s grace period, with bots driving only when bot fill is enabled (otherwise idle). Hide empty rooms from discovery immediately. Reconnect within that period receives current full state, even if the round has ended. If nobody returns, stop timers, release tokens and `dispose()` the simulation. Test this period with no incoming messages in M1.
- Pass host authority to the next connected player when the host disconnects or leaves; a returning former host does not reclaim it automatically. Never grant two sockets authority for one seat.
- A deployment or runtime restart can discard the whole in-memory match. A changed room epoch or expired room returns the client to a fresh lobby with a visible explanation. Persisted recovery is out of scope; protocol-version equality alone does not imply the old match survived.

## Milestones

Every implementation PR ends with `npm run check` passing, including server types/build once introduced. Add focused deterministic tests for changed behavior and run `npm run check:browser` for input, menu or rendering changes. Long hosting and latency measurements remain manual evidence, outside normal CI. Sizes are relative: S, M, L.

Before the first simulation edit (M2 PR A), capture a fresh `npm run validate` baseline. Compare subsequent simulation changes with that baseline, allowing only `wallSeconds`. Inspect and restore only the generated validation output from this task. Preserve the original seeded behavior, not merely the results of an already-refactored parent revision.

Order: M1 hosting experiment and M1b local latency spike → M2 simulation/view boundaries → M3a early playable integration → finish M3/M4 → M5 full client flow → M6 friends playtest. M1b needs no server and no simulation changes, so it can run alongside M1. Share scheduling/timing code between the experiments and eventual host where practical. A minimal read-only presentation boundary may be pulled forward from M2 to make M1b faithful; capture the baseline first. The two-browser latency cases begin in M3a and are repeated in M5.

### M1: Server skeleton and hosting gate (M)

- Create `server/` with a workerd physics entry and Rapier loader and a Durable Object running a bot-driven Team Battle at 60 Hz. Prototype a 20 Hz JSON stream with rounded numbers containing tanks, projectiles, moving cover/debris and events, plus representative full-state bursts. Position-only traffic is insufficient evidence for the final workload.
- Add `wrangler`, `esbuild` as a direct dependency, and `@cloudflare/workers-types` as dev dependencies, then run `npm ci`. Add `server:dev`, `server:build` and `server:deploy`; ensure the latter two build the current source before use.
- Add `server/tsconfig.json` and include it in `npm run typecheck`. Replace the inline `tsc --noEmit` in `build` with that combined type check, and add `server:build` to `npm run check`. Thus the gate checks both targets once through `build` and also verifies the Workers bundle; extending an otherwise unused `typecheck` script would not suffice. Lint server source; exclude `server/dist/` and Wrangler outputs from Git, Prettier and ESLint.
- Use the same bounded scheduling policy intended for M4. Drive the room from one 50 ms timer: each callback runs the fixed 1/60 s steps owed by elapsed time (normally three) and then sends one snapshot. That gives a third of the wakeups of a 60 Hz timer with the same visible latency, and every snapshot lines up with a step batch. Allow at most six steps in a callback and a 250 ms maximum accumulated debt. Retain debt between callbacks; if it exceeds that bound, terminate the round with an overload reason rather than silently skipping physics or entering an unbounded catch-up loop. Verify the runtime clock advances correctly during timer-driven and quiet periods.
- Deploy to `workers.dev` on Free. Include one deliberate timer stall to verify the overload path separately from normal-load measurements. The expected load is 2–4 friends, so size the normal-load matrix to it:
  - repeated cold room creation;
  - one quiet client;
  - one 4-client room for 15 minutes, including explicit collapse fixtures;
  - a short 8-client spot check;
  - two simultaneous rooms;
  - burst destruction on every map;
  - the 30 s no-client reconnect grace period.
- Record startup/initialization time, CPU-limit errors, resets, tick debt and simulated-versus-wall time, serialization cost, memory peaks, input rates, bytes per client, full-state sizes, and account quota consumption. Local elapsed timings are not Cloudflare CPU-accounting measurements; use available deployed metrics and `wrangler tail` alongside them.
- Retain all slow samples and record exact scripts, seeds and configurations. Repeat the 4-client run and the 8-client spot check with the final M3 serializer in M4.
- Deployment needs an existing authorized Wrangler session or an appropriately scoped Workers token; verify available credentials rather than assuming the Pages CI token grants access. A paid subscription change remains a separate owner decision.
- **Done when:** the selected plan passes the 4-client run and the 8-client spot check without unexpected resets/CPU-limit errors or sustained tick debt, cleanup and deliberate overload behave correctly, and usage evidence is recorded. Merely choosing Paid does not pass this gate. Stop and reconsider hosting if the timer/CPU model cannot reliably support the workload.

### M1b: Local latency spike (S)

The open question is whether driving and combat feel acceptable without prediction when authoritative feedback arrives one round trip late, in 20 Hz samples. Answer it before committing to the complete protocol/client. Smoothing can hide stepping without removing response delay; 100–150 ms RTT may require prediction, which remains a hypothesis until tested.

- Add a dev-only URL option to the local game controller, off by default. Sample inputs at the selected rate, delay them by half the simulated round trip, and run the same 50 ms batched fixed-step scheduling policy as M1. Sample authoritative presentation state at 20 Hz and deliver it after the other half. The Simulation, its fixed step and its RNG order are unchanged; queues, sampling, and display overrides live outside it.
- Draw the hull and camera from those samples under each candidate policy: the newest sample, continuous render-only smoothing, and bounded velocity extrapolation with correction. Do not extrapolate indefinitely through walls or across death/respawn. Keep turret aim immediate, drawn from the displayed hull toward the pointer; the delayed command carries the aim point, as the protocol does. Opponents, projectiles, spatial sounds, impacts, deaths, and destruction must use the intended delayed display timeline. Direct access to current simulation state for those paths would make combat feel unrealistically responsive. Sending aim points corrects the firing origin but does not compensate for aiming at old target positions.
- Test 0, 50, 100 and 150 ms of added round trip, a case with up to 30 ms variable delay that preserves order, and a short stall. Compare 20 and 30 Hz input sampling. Watch the hull and camera while starting, stopping, turning and colliding, and while shooting at moving targets. Record button-to-movement, button-to-shot, and shot-to-hit-feedback delay separately, including sampling/batch phases and any cosmetic feedback policy.
- **Done when:** the tester records which cases feel acceptable, the local hull/camera do not visibly jump at 20 Hz under the selected policy, and the selected smoothing policy, input rate and latency envelope are written down.
- If controls are unacceptable at the intended playtest latency, bring own-tank prediction and reconciliation into v1 before building the client. Its protocol goes in M3 (input sequence acks already support it) and its client in M5 PR A. Otherwise revise hosting or scope. A polished lobby is not a substitute for this gate.
- Prediction is a substantial alternative implementation path. Before M3, specify how 20/30 Hz messages represent 60 Hz command timing, which tick consumes each command, and exactly what an acknowledgement retires. A highest input sequence alone does not define replay duration. Validate replay across partial consumption, batching, collisions, and control-epoch changes. Use an isolated prediction world and keep damage and outcomes authoritative.

### M2: Team Battle simulation and presentation boundaries (L, three PRs)

Keep single-player Team Battle and Solo Assault behavior intact and compare against the pre-refactor baseline above. Add focused comparisons for maps/modes/difficulties outside the default validation workload.

**PR A — seats and control:**

- Add `stepWith(commandsByTank)` with explicit seat/driver state, starting with a focused independent-movement test for two player-legal tanks. The legacy `step(command, autoplay)` retains its behavior, defaults and seeded draw order. A connected seat with no valid command is idle; an explicitly bot-driven seat uses its existing brain.
- Introduce the tank life generation separately from `deaths`, moving each current reader deliberately. In single-player the new counter must advance exactly as `deaths` does, so seeded results stay identical. Today `tank.deaths` doubles as the life counter:
  - shots and mines record it as `ownerLife`, and TOW rounds as `targetLife`;
  - XP and kill credit, burnout and seeded wreck motion read it;
  - the readers are in `weapons.ts`, `mines.ts`, `projectiles.ts`, `veterancy.ts`, `damage.ts`, `combat-record.ts` and `wrecks.ts`.
- Keep seat ownership, current driver and local viewer distinct. Temporary bot takeover does not change a reserved player's kind, team, health modifiers or score. Fill-bot difficulty follows the multiplayer rule below, independent of the viewer.
- Team reset fills six slots per team, at most eight human seats overall. Respawn uses the seat's kind instead of the single `humanKind`. Solo still creates one human and keeps its existing spawning, targeting and end conditions.
- In multiplayer, wreck placement uses the existing headless fixed bounds and debris cleanup considers every occupied player seat. Keep single-player camera-dependent wreck placement local to that mode; no remote viewer can alter authoritative wreck physics.
- Test independent commands, one-shot consumption, absent commands, bot takeover, seat limits, per-seat respawn choices and old-life ordnance credit. Preserve existing single-player tests.

**PR B — match settings and event data:**

- Replace shared mutation in `speed-tuning.ts` with per-simulation speed options. Base `VEHICLES`/`WEAPONS` values remain unchanged. Thread the effective values through movement, firing and CCD configuration; preserve local tuning behavior, including already-moving shots. Multiplayer v1 locks speeds to the checked-in defaults and ignores browser speed settings.
- Run two differently configured simulations in one process, interleaving steps, and compare each with its isolated run. Changing one match must not affect the other or a later single-player match.
- Keep the existing single-player combat recorder bound to its player. Disable full recap recording for multiplayer and use existing per-tank kill/death counters for its scoreboard. Audit record writers in `damage.ts`, `weapons.ts`, `pickups.ts`, `projectiles.ts` and `tank-lifecycle.ts` so a second human cannot pollute the first player's record. Do not build full per-tank reports yet.
- Construct events from explicit plain fields. In particular, `damage.ts` must not spread a drum/cover or mine into `explode()` events. Move viewer-dependent "YOU" text into the UI using entity ids. Test decoded JSON meaning, including indestructible rubble, using the protocol's explicit wire representation.

**PR C — viewer and render state:**

- Complete the read-only render-state contract and local simulation adapter. Move camera, HUD, local audio and touch HUD selection to an explicit viewer tank id in `presentation.ts`, `ui.ts`, `ui-markup.ts`, `game.ts`, `touch-controls.ts` and `touch-mode.ts` as needed. Keep single-player recap entry points working for their original player.
- Supply poses and appearance/HUD values instead of Rapier body access. Keep local actions and legacy local-only feedback paths explicit in the game controller. Preserve model/resource ownership and track history; rendering must not mutate authoritative physics.
- Test different multiplayer viewers without changing authoritative simulation results, and preserve the existing local camera/wreck behavior in single-player. Run the existing browser suite and inspect camera, respawn, HUD, touch input and effects; check reset/long-run resource bounds for any new buffers.
- **Done when:** all three PRs pass their gates, fresh seeded single-player results match the baseline apart from timing, and no multiplayer Solo or full-report work has been introduced.

### M3a: Early playable integration (M)

- Pull forward the smallest coherent parts of M3/M4/M5: one map, two humans with bots filling the roster, one round, full baselines, movement, shooting, death, and respawn through the real render mirror and WebSocket host.
- Use the intended identities, serializers, and transport-independent host; do not create a disposable second multiplayer implementation. A dev fixture is sufficient before the production-shaped lobby/results UI exists.
- Repeat M1b's driving and combat cases with two actual browser contexts, including delayed lifecycle effects. Resolve responsiveness and missing render-state fields before completing late-join, host-transfer, and lobby polish.
- **Done when:** both players independently move, fight, die, and respawn in the same authoritative round and recorded latency results support the chosen prediction policy. This does not waive the later lifecycle, reconnect, or hosting gates.

### M3: Match host, complete protocol and deterministic harness (L, three PRs)

**PR A — schema and host:** implement `protocol.ts` and `match-host.ts`, including validation, seats, input leases, action queues, input-sequence acks, room/round/life/control identities and host-only transitions. Keep transport/runtime APIs outside `MatchHost`.

**PR B — state mirror:** implement `mirror.ts`, full baselines, ordered deltas, final sleep/wake poses, explicit removals, resync and render-state projection. Use server-issued entity ids and complete decoded state rather than constructing a second gameplay simulation from the seed.

**PR C — timing and failure harness:** implement `interpolation.ts` and a fixed-clock `MatchHost` plus fake clients that exchange serialized JSON. Use the M1b smoothing choice. Compare authoritative mirrors at matching ticks, and test delayed display poses separately against their intended display time.

**Round-trip test.** This is the main replication test. Run a seeded headless Team Battle through `MatchHost` on each map, including an explicit collapse fixture. At every snapshot, serialize to JSON, apply the message to a mirror and project its render-state. Compare that with the render-state projected directly from the authoritative `Simulation` at the same tick, within the wire rounding.

- Also start late-joining mirrors from full baselines at several points, such as just after destruction and after bodies settle, and assert that they converge to the same render-state.
- This covers real tank state, destroyed cover and stumps, collapse rubble, sleeping and waking bodies, fragment cleanup and capacity removal, pickups and respawns as they occur in matches.
- It also fails whenever a field that rendering reads is not replicated, now or in a later feature. That is the ongoing cost of a client that does not simulate.
- Keep a short window per map in `npm test`. Full-length matches are a manual script.

Targeted tests cover what the round trip cannot observe:

- death/respawn with the same tank id, interpolation-history reset, round changes, content-version mismatch and room restarts; out-of-sequence snapshot detection without partial application;
- continuous input between messages, stale and future input, lease expiry to idle and then to bot, reclaim through a new control epoch, suspension/resume, action-queue bounds and expiry, and queued actions cleared across death/reconnect;
- disconnect and reclaim, both clients disconnecting together, the room's grace timeout, seat expiry/reuse, old-socket rejection and host transfer;
- the event cursor across full-state boundaries, late joins without historical sound bursts, events on the display timeline, and lifecycle changes surviving coalesced rendered frames;
- delayed visible creation/death/removal synchronized with spatial effects, and launch/impact presentation for projectiles that live entirely between snapshots;
- slow clients and bounded queues: request a full resync or disconnect rather than growing memory without limit; repeated rounds and disposal release room resources;
- explicit encoding of non-finite internal values such as `Infinity` hp, and no physics-object leakage.

Measure actual UTF-8 serialized bytes per client per second, steady/burst snapshot sizes, event traffic, acknowledgements and full states on every map, with the wire rounding applied. Set documented budgets from those measurements and preserve representative fixtures for fast regression checks. Binary/Colyseus estimates are not acceptance thresholds for JSON.

- **Done when:** the harness and the round-trip test run deterministically in `npm test`, all lifecycle/failure cases pass, wire-size budgets are recorded, and the fixture renders through the same mirror used by the eventual client.

### M4: Complete Durable Object server (M)

- Wrap `MatchHost` in the M1 server and route `/room/CODE` to it. Keep M1's validated 50 ms timer, fixed-step accumulator, catch-up bounds and overload behavior. Serialize each snapshot body once for all clients.
- Implement seat/room grace periods, token invalidation, host transfer, explicit restart messages and disposal. An explicit final departure stops immediately; a disconnected-empty room stops after its grace period; idle lobbies/results also need bounded lifetime and timer cleanup.
- Use exact permitted production/dev/Pages origins plus explicit localhost development origins, not a blanket `*.pages.dev` allowlist. Enforce eight-player/six-per-team limits, room-code format, allowed phase transitions, protocol/content versions, and bounded message/connection/room-creation rates. Origin checking is not seat authorization.
- Repeat M1's load and quiet-room tests with the completed protocol and actual serializer, including late-join/full-state bursts and slow readers. Verify both Free/selected-plan limits and observable bandwidth; update the evidence and budgets if the prototype estimates were low.
- **Done when:** two Node clients complete rounds through `wrangler dev` and the deployed server, the 4-client run and 8-client spot check pass, disconnect/reconnect and deliberate restart tests have the specified outcomes, and normal workloads do not cause unexpected room loss.

### M5: Client multiplayer mode (L, three PRs)

**PR A — connection and playable integration:**

- Add `connection.ts` and a network session in `game.ts`: collect `VehicleCommand` input, queue actions, send at the agreed rate, apply snapshots, and render the mirror. Do not call `sim.step()` on a network client. Keep local single-player startup and controls intact.
- Use the tested smoothing policy. Remote poses, projectiles, spatial events and presentation time share a coherent delayed timeline; the local hull uses the M1b policy and its turret follows the pointer immediately. Send the pointer's ground aim point, or the stick angle for touch. Bound projectile extrapolation and correct authoritative impacts. Tracks compare rendered frame-to-frame positions. Clear histories on new lives, full resync and resume.
- Carry the M1b delay harness over as a dev-only `connection.ts` option. It splits the added delay across both directions and preserves WebSocket ordering. Use it to repeat the M1b cases with two browsers, plus a stalled connection and a hidden-tab/resume cycle. Record actual RTT separately from the added delay.
- Use `VITE_MULTIPLAYER_URL`; allow `?server=` overrides only in local/dev builds. Avoid changing production deployments or activating multiplayer there during this phase.
- Enter through a dynamic multiplayer import, including when following a room link. Verify the normal production single-player path requests no multiplayer chunks, starts no networking activity, and retains its original startup/loading behavior. Keep dev latency/diagnostic code behind dev-only dynamic imports.

**PR B — lobby and results:**

- "Play with friends" opens a room browser with saved/random name, Auto/manual team and tank choices. Listings show players, map, bot mode, time and score. Select and Join an existing room; Create starts the selected map immediately, alone if humans-only is enabled. Share `?room=CODE` from the game menu. Show loading, full/team-full, incompatible-build, reconnecting and room-reset states.
- Players choose team/kind in the lobby; the host picks map/difficulty and starts. Store display names locally. Enforce the same choices and limits on the server, including the bot-only HMMWV restriction.
- Show kills/deaths per participant for the current round, keeping a departed participant's row separate if someone else takes that tank slot. Do not credit a new player with the previous player's counters. Multiplayer does not write single-player personal-best records. Host can start another round from the lobby.

**PR C — visibility, menus and browser checks:**

- The local menu never pauses the match. All players can LEAVE; END BATTLE is additionally host-only. Opening the menu or hiding the tab clears local controls and sends `suspend`; the server's input timeout covers a message that cannot be sent. Bots drive suspended seats.
- Keep the socket if the browser permits, but do not depend on hidden-tab timers. On returning, obtain a fresh baseline/control epoch, discard old queued actions/effects and resume from current state. Show the bot-takeover/reconnect state clearly.
- Touch still emits `VehicleCommand`, but its viewer binding, mine cooldown, action queue and visibility/resume behavior need integration and real touch coverage; do not assume it needs no changes.
- Add `scripts/multiplayer-check.mjs` using the existing browser helpers. Drive two Chrome contexts through create/join, independent input, combat, respawn, hidden/resume, simultaneous reconnect, host departure, results and a second round. Include team-full and incompatible-version failures. Check real pointer input for pointer-sensitive controls and retain all single-player browser checks.
- **Done when:** both browsers finish the complete flow against `wrangler dev`, the M1b latency cases still behave as recorded with two browsers and the real UI/rendering, remote motion and spatial effects agree, and single-player checks pass.

### M6: Playtest with friends (S)

- Additional automated work does not require another home network: run bounded synthetic players from a separate Cloudflare test Worker/Durable Object against the public room endpoint; mix them with a local browser and record disconnects, reconnect identity, full/delta validity and bandwidth. Keep the runner manually invoked, authenticated and duration/player capped. This tests a client outside the local machine, but Cloudflare-to-Cloudflare traffic does not establish residential/mobile network behavior or human control feel. The runner is a follow-up, not deployed yet.
- Run the server on `workers.dev` and publish the client to the dedicated dev site with `npm run deploy:dev`. Keep production unchanged. Check public game/test assets and build metadata after publishing.
- Record actual RTT per player and region, input-to-visible delay, jitter, bandwidth, request-quota use, deployed CPU/limit metrics, tick debt, full-state bursts, errors and player feedback. Compare with M1b's tested latency envelope rather than reporting only same-machine success.
- **Done when:** several 5-minute rounds with 2–4 players on different networks finish without unexpected desync or room loss, hidden-tab and simultaneous-reconnect tests recover, deliberate server restarts return everyone clearly to the lobby, and responsiveness is acceptable. Prioritize follow-ups from the recorded results before considering production rollout.

## After v1

Choose from the playtest results:

- **Own-tank movement prediction, if M1b allowed it to remain deferred:** use a separate local prediction world with the authoritative movement rules and an appropriate collision-state mirror. Reconcile against the input-sequence acks already carried by snapshots and replay unconfirmed movement. Keep gameplay outcomes authoritative and feed predicted poses through render state; the render mirror itself does not become a simulation. Validate collisions, moving cover, knockback and respawn rather than assuming `driveTank` alone is sufficient.
- **Binary snapshots,** if measured JSON bytes or serialization cost justify them. Reduce input message frequency separately if server load is the concern, retaining liveness and one-shot guarantees.
- **Per-player battle report,** reusing the existing single-player recap.
- **Co-op Solo Assault.**
- **Production rollout:**
  - point both Pages builds at the server;
  - add a GitHub Actions deploy for the VPS server with a deploy-only SSH key.
- **Colyseus,** only if hand-written network code becomes the bottleneck.
- **WebTransport,** only on a host with an open UDP port.

## Risks

| Risk                                                                      | Mitigation                                                                                                    |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Actual Free CPU/timer limits do not support a continuous match            | Confirm applicability and run M1 on the deployed plan, including quiet periods; repeat on any chosen fallback |
| Driving or camera motion feels delayed or steps at snapshot frequency     | M1b local latency spike before the refactor; prediction moves into v1 if it fails; two-browser recheck in M5  |
| Missing removals, sleeping poses or old-life state leave the client wrong | Explicit lifecycle deltas, life ids, atomic full baselines, sequence checks and M3 failure tests              |
| A field rendering reads is never replicated, now or in a later feature    | M3 round-trip test comparing mirror and authoritative render-state every snapshot                             |
| Spatial effects appear before their delayed targets reach an impact       | Tick-stamped events and a shared remote display timeline, verified visually                                   |
| Shots miss because aim was computed from a lagging displayed hull         | Send the pointer's ground aim point; the server computes the angle from the authoritative hull                |
| Stale input keeps firing or applies actions after respawn                 | Input leases, control epochs, and bounded, expiring action queues                                             |
| A brief connection hiccup hands a connected player's tank to a bot        | Silence idles the tank first; the bot drives only after 5 s, on suspend or on socket close                    |
| Everyone briefly disconnects and loses the match                          | Bounded 30 s empty-room grace period with bot control; full state on return                                   |
| A deployment/runtime restart loses in-memory state                        | Distinct room epochs and a clear return to lobby; avoid planned deployments during playtests                  |
| The refactor changes single-player results or leaks balance across rooms  | Fresh baseline comparisons, unchanged Solo rules, per-simulation settings and interleaved-room tests          |
| JSON traffic or client queues are larger than estimated                   | Measure actual encoded traffic/full states; bound queues and resync/disconnect slow clients                   |
| The VPS runs out of CPU, memory or monthly transfer                       | Watch `npm run vps:stats` and the minute summaries; bound rooms, compress traffic, or move to a larger plan   |
| Friends far from the VPS get high RTT                                     | Record per-player RTT; pick the VPS region for the expected players, or add hosts in other regions            |

## Gameplay defaults and decisions still requiring evidence

- **Players per room:** up to eight humans, six slots per team. New rooms default to humans-only; optional bots fill a 12-tank roster. A reconnect reservation counts against both room and team capacity.
- **Teams:** Auto selects the side with fewer human seats, including reconnect reservations; ties choose Blue. Explicit team choices are honored subject to capacity. Players may change teams between rounds, with no mid-round switching. Eight humans cannot all play on the same six-seat team.
- **Player bonus:** every human seat keeps the single-player 1.2× fire rate (`PLAYER_FIRE_RATE_MULTIPLIER`) in v1. In player-versus-player with uneven human counts the bonus compounds the larger human side's advantage. Collect team-balance feedback in M6 before changing it.
- **Kinds:** players choose scout, balanced or heavy. HMMWV/TOW remains bot-only. A late join claiming a bot slot starts the chosen kind at a fresh respawn with normal protection; invalidate the previous life so its ordnance cannot earn XP for the newcomer. Reconnect to a reserved seat keeps its ongoing life and kind.
- **Late join:** allowed if a non-reserved human seat is available. Keep player scoreboard identity separate from tank-slot identity, so a newcomer does not inherit a departed player's kills/deaths. Team scores already earned stay intact.
- **Host leaves:** pass authority to the next connected player in join order; reconnect does not preempt the new host. If nobody remains connected, select a host on the first valid return within the grace period.
- **Difficulty:** in multiplayer, all fill-bot tanks use the chosen difficulty on either team. A bot temporarily driving a reserved player seat retains that seat's player balance. Single-player keeps its existing enemy-team difficulty rule and Solo tuning.
- **Speed tuning:** multiplayer v1 uses the checked-in defaults; localStorage tuning stays single-player-only and per simulation. Exposing host speed controls is deferred.
- **Evidence gates:** the hosting plan and exact local-hull smoothing/prediction policy remain conditional on M1/M1b. Record the chosen policy, acceptable latency envelope and actual JSON budgets before declaring the relevant milestone complete.
