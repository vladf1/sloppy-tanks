# Pilots

Pilots are headless players for trying multiplayer against something that plays like a
person. Each one joins a room the way a browser tab does, through the game's own network
client (`sloppy_core::net::client::NetworkClient`), and the fill bots' brain
(`sloppy_core::sim::ai::bot_command`) plays its tank. A pilot sees the room the way a
player in its seat would: other tanks drawn late by the view delay and its own tank
predicted ahead. It aims and fires at what that view shows, so a pilot running in Seattle
tests hit registration, prediction and lag compensation from Seattle.

The traffic bots in `bots/` are different. They drive at random to load a server and read
nothing but message headers.

## Missions

| Mission       | What the pilot does                                                                                                                                                                                                                    |
| ------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `skirmish`    | Plays like a fill bot on whichever team the room gives it.                                                                                                                                                                             |
| `hunt:NAME`   | Takes the other team from the player named NAME (case-insensitive) and goes after them first, wherever they are. It always knows where they are, the way a bot knows who just hit it. It still fires only when it has a line of sight. |
| `hunt`        | The same, for the nearest player (a person or another pilot, never a fill bot) on the other team.                                                                                                                                      |
| `escort:NAME` | Takes NAME's team, follows them (the support bots' escort), fires at whatever they hit and turns on whoever hits them.                                                                                                                 |

A pilot that is on the wrong team changes side. Between battles it makes a team choice.
During a battle, when choices are locked, it gives up its seat and joins again with the
team it needs. `skill` (`easy`, `normal`, `hard`) tunes reaction, aim error and reload,
as a room's difficulty does for fill bots. `kind` picks the tank. `style` (a bot role:
`scout`, `guard`, `sniper`, `heavy`, `minelayer`, `support`, `artillery`) picks range,
sight, cadence and favourite ammunition. By default the role follows the tank, and escorts
play `support`. A pilot that hosts its room starts the next battle 15 seconds after the
results, so a room it made never strands people.

Some human touches sit on top of the brain:

- A fire decision holds the trigger for 120 ms, like a click.
- The turret wobbles slightly, by skill.
- A mine or an ammunition change is asked for once and then waits a round trip.
- No hit-markers, banners or wall hacks. The only exception is the hunt's knowledge of
  where its quarry is.

## How it works

```
server ──binary snapshots──▶ NetworkClient ──RenderState (displayed frame)──▶ WorldModel
   ▲                              │                                               │ sync
   └────── input JSON ◀── LocalInput ◀── Cockpit (mission + human layer) ◀── bot_command
```

- `src/pilot.rs`: `Pilot` drives `NetworkClient` the way the page does. It answers
  `PrepareArena` by building its `WorldModel`, answers `ShowBaseline` at once, gives its
  socket ids a generation (so a rejoin never mixes up old and new sockets) and plays
  hosting rounds. Its `Cockpit` turns each displayed frame into the next frame's
  `LocalInput`. Missions work through the brain's memory of being hit: a hunt "hears" its
  quarry every frame (`brain.attacker`, `attacked_from`, `alarm`), and an escort marks its
  leader's targets and attackers that way. Escorting also marks every other ally as
  `Support` in the model, so the support role's "follow the nearest non-support ally"
  follows the leader.
- `src/world_model.rs`: `WorldModel` is a `Simulation` of the room's arena, built by the
  host's own room setup (`create_multiplayer_simulation` with an empty humans-only
  roster), so cover, ids, pickups and level rules match the host. It is never stepped:
  every frame it places tanks and movable cover where the scene draws them, takes fallen
  cover out (a tree keeps its tank-only stump), restores rebuilt cover, adds cover the
  layout lacked (rubble), patches navigation, and refreshes Rapier's queries with
  `detect_collisions`. The model's physics world is rebuilt once without the host reset's
  step, because bodies added to a stepped world and only ever refreshed by the collision
  pipeline trip Rapier's island bookkeeping.
- `src/report.rs`: the battle tally (hits, kills, hits on and from players, trigger pulls,
  team changes) and the network feel, using the browser's statistics (RTT, fire-to-flash,
  view delay, hit offset, shells drawn hitting that the host flew past, prediction
  corrections, late batches).
- `src/fleet.rs` and `src/abi.rs`: the pilots behind JSON calls, exported from Wasm with
  no imports, for the bots Worker (`bots/pilot-engine.ts`).
- `src/command_line.rs` and `src/native/`: the command line (tokio, tokio-tungstenite and
  rustls with the server's pure-Rust crypto, a directory lookup over hyper).

## Run

From this machine, against any server:

```sh
pnpm run pilot -- --server ws://127.0.0.1:8787 --create village --pilots 3   # a local server
pnpm run pilot -- --room ABCD2345 --hunt vlad --pilots 2 --skill hard         # a production room
pnpm run pilot -- --room ABCD2345 --escort vlad --environment dev             # a dev-server room
pnpm run pilot -- --help
```

Without `--server`, the server directory names the room's server, or for `--create` the
server a new room opens on. The command prints a line per pilot every 5 seconds. When its
time is up (`--minutes`, 10 by default), on Ctrl-C, or once every connection has given up,
the pilots send `leave` and their reports print as JSON (`--report FILE` writes them to a
file).

From Cloudflare regions, through the bots Worker (see `bots/README.md`):

```sh
pnpm run bots:deploy                                     # builds the Wasm engine, then deploys
pnpm run bots -- start wnam 2 --room ABCD2345 --hunt vlad
pnpm run bots -- start weur 3 --brain --host --map harbor
pnpm run bots -- status
```

**Content versions:** pilots are game code. `pnpm run pilot` and `pnpm run pilot:build`
stamp `SLOPPY_CONTENT_VERSION` as the browser build and the server do. A server whose
content version differs refuses the pilots with "Game updated". Run or deploy them from the
commit the target server runs. The bots Worker reports a mismatch as a region error naming
both versions.

## Tests

```sh
cargo test -p sloppy-pilot        # unit and integration tests, about 10 s in debug
pnpm run server:build && pnpm run pilot:build && node --import tsx --test tests/pilots.test.ts
```

- `tests/integration/missions.rs` wires pilots and the real `MatchHost` in memory with one
  clock (`support/mod.rs`, like `crates/core/tests/integration/net_client.rs`) and checks
  three behaviours:
  - a pilot creates a room, drives, and the host fires and lands its shots;
  - a hunter takes the other side and hits its player;
  - an escort switches side mid-battle (leave and rejoin) and stays within 16 m of its
    player.
- `tests/integration/world_model.rs` checks three things:
  - the model has the host's cover ids and pickups on every map;
  - synced tanks stand where the scene draws them;
  - fallen and rebuilt cover open and close sight lines and routes.
- `tests/pilots.test.ts` (root) runs the Wasm engine and `bots/pilot-seat.ts` against the
  real server binary over real sockets, and pins the report's JSON shape that the control
  page reads.

## Cost

These are rough measurements from a shared Linux container. The pilots ran in Node's V8
with the release (size-optimized) Wasm, at 30 frames a second, with the server on the
same machine:

| Measurement                      | Value                             |
| -------------------------------- | --------------------------------- |
| CPU per pilot (village)          | about 60–80 ms per second of play |
| CPU for 8 pilots in one instance | about half a core                 |
| Wasm memory, 8 pilots            | 18 MiB                            |
| Engine size                      | 2.5 MB (870 KB gzipped)           |

A speed-optimized build (`opt-level = 3`) measured 20–30% less CPU in two of four runs,
which is within this machine's noise. That is not proven yet. Most of the cost is
probably the client's own hull prediction (a Rapier world stepped at 60 Hz, as in the
browser) plus the model's query refresh.
