# Traffic bots and pilots

Network-load players for the multiplayer server, and [pilots](#pilots) that actually
play. A traffic bot joins through the public room protocol like a browser tab but
never reads the map: it drives in random directions, sweeps its turret, fires, and
sometimes drops mines or switches ammunition. Bots are named `bot-<region>-<n>`,
pilots `pilot-<region>-<n>`, and both occupy real seats.

The `sloppy-tanks-bots` Worker (`https://sloppy-tanks-bots.vova145.workers.dev`)
plays on production's game servers, which it finds in the server directory
(`DIRECTORY_URL` and `SERVER_ENVIRONMENT` in `wrangler.jsonc`): each swarm takes the
least busy one and joins or creates rooms on standard maps there. It is separate from the game server and both Pages
sites, and runs on Cloudflare so bots can connect from many regions.

## How they live

Each region runs one `BotSwarm` Durable Object created with a
[location hint](https://developers.cloudflare.com/durable-objects/reference/data-location/)
(`wnam enam sam weur eeur apac oc afr me`). It holds up to 32 bot sockets,
reports its actual colo, and reads the server's `/health` for the protocol and
content versions, so a server redeploy does not need a bot redeploy.

Idle bots browse `/rooms` every 10 seconds and fill compatible rooms with free
seats, most-occupied first. With `host`, a bot creates a five-minute room when
nothing has space, and the rest join it. A bot that ends up as the lobby host
starts the next round after 15 seconds, so people are never stuck waiting on it.
A dropped socket reclaims its seat with its token. Full, ended or incompatible
rooms are skipped for a minute.

Every run has a deadline (default 5 minutes, maximum 6 hours). A 30-second
alarm stops the run at the deadline and brings the bots back if a deploy or
runtime restart evicted the object. Stopping sends `leave`, which frees seats
immediately.

## Use

```sh
pnpm run bots:deploy                    # deploy after changing bot code
pnpm run bots -- start weur 4 --host    # --minutes 5 --room CODE --per-room 1
pnpm run bots -- start apac 2           # join whatever rooms are open
pnpm run bots -- status
pnpm run bots -- stop apac              # or no region to stop all
```

The root URL serves a control page with the same actions. There is no
authentication: anyone with the URL can start and stop bots, bounded by the
32-bot and 6-hour limits per region. Local
development: `pnpm exec wrangler dev --config bots/wrangler.jsonc` with `DIRECTORY_URL`
overridden to a local server (`http://127.0.0.1:8787`), which lists itself.

## Pilots

With `--brain` (the control page's **Pilots** box), a region runs up to 8 pilots
instead of traffic bots: players driven by the bot brain through the browser's own
network client ([crates/pilot](../crates/pilot/README.md)), compiled to Wasm
(`bots/generated/sloppy_pilot.wasm`, built by `pnpm run pilot:build` and by
`bots:deploy`). The pilots see the room with the region's latency, so they test how the
game feels from there: hit registration, prediction and lag compensation.

```sh
pnpm run bots -- start wnam 2 --room ABCD2345 --hunt vlad     # hunt vlad in that room
pnpm run bots -- start weur 1 --room ABCD2345 --escort vlad   # fight beside vlad
pnpm run bots -- start apac 3 --brain --host --map harbor     # make a room, play there
pnpm run bots -- start enam 4 --brain                         # join the fullest open room
pnpm run bots -- start wnam 2 --room ABCD2345 --hunt vlad --skill hard --kind heavy --env dev
```

- **Room.** All of a region's pilots play one room: the `--room` code (on the server the
  directory lists it on), a new room (`--host`, on `--map`) whose first pilot creates it
  and hosts it, or else the fullest open standard room.
- **Missions.** `--hunt NAME` takes the other side from NAME and comes after them.
  `--mission hunt` goes after the nearest player on the other team. `--escort NAME` takes
  NAME's side and follows them. Plain `--brain` plays `skirmish`. Pilots switch sides by themselves: a team choice between battles, or leave
  and rejoin during one.
- **Status.** `status` and the control page show each pilot's phase, team, K/D, hits on
  and from players, RTT, view delay, fire-to-flash time, hit offset, prediction
  corrections and phantom hits (shells drawn hitting that the server flew past).
- **Recovery.** A pilot whose connection gave up rejoins after 15 seconds, taking back a
  seat the server still holds. A room that ended or refuses the pilots makes the region
  choose again. A Rust panic traps the Wasm instance: the region starts a new one and
  every pilot takes its seat back (`restarts` in the status).
- **Content version.** The engine is game code, stamped with the checkout's content
  version. A server running another version refuses its pilots, and the region reports
  both versions. Deploy the Worker (`pnpm run bots:deploy`) from the commit the target
  server runs; the traffic bots have no such tie. `--env dev` (`environment` in the API)
  plays on the dev server's rooms instead of the Worker's `SERVER_ENVIRONMENT`.
- **Cost.** A pilot runs the browser client's hull prediction and a model of the arena
  every frame (30 a second). Measured in Node, a pilot takes about 60–80 ms of CPU per
  second of play, and 8 pilots about half a core and 18 MiB of Wasm memory.

## Cost

Nothing runs between runs: no cron, and a stopped object has no alarm. While a
region runs, its object stays awake and bills duration (128 MB x wall time)
because its outbound sockets cannot hibernate. That's about 460 GB-s per region-hour.
Check the account's plan allowances before long or wide runs; current limits are
on the
[Durable Objects pricing page](https://developers.cloudflare.com/durable-objects/platform/pricing/).
On the game server, a bot costs the same as a human player: about 20 inputs a
second in, and the room's snapshot stream out (binary deltas under permessage-deflate:
roughly 3–6 KB/s per bot in a standard room and up to about 45 KB/s in the Scrap Yard,
which counts against the VPS's monthly transfer).

`/api/status` queries all nine region objects, and the control page polls it
every 10 seconds while visible. Close the page when you are not watching.
