# Load bots and pilots

Load bots put network load on the multiplayer server, and [pilots](#pilots) actually
play. A load bot joins through the public room protocol like a browser tab but never
reads the map: it drives in random directions, sweeps its turret, fires, and sometimes
drops mines or switches ammunition. Load bots are named `bot-<region>-<n>`, pilots
`pilot-<region>-<n>`, and both occupy real seats. One control page and API start and
stop both; each region can run load bots and pilots at the same time.

The `sloppy-tanks-bots` Worker (`https://sloppy-tanks-bots.vova145.workers.dev`,
`worker.ts`) serves the control page and API and runs the load bots. It hands pilot runs
to the `sloppy-tanks-pilots` Worker (`pilots-worker.ts`, `pilots.wrangler.jsonc`) over a
service binding; that Worker has no public address and deploys on its own. The load bots
play on production's game servers, which they find in the server directory
(`DIRECTORY_URL` and `SERVER_ENVIRONMENT` in `wrangler.jsonc`): each swarm takes the
least busy one and joins or creates rooms on standard maps there. Both Workers are
separate from the game server and both Pages sites, and run on Cloudflare so bots can
connect from many regions.

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
pnpm run bots:deploy                    # deploy after changing the load bots or control page
pnpm run bots -- start weur 4 --host    # --minutes 5 --room CODE --per-room 1
pnpm run bots -- start apac 2           # join whatever rooms are open
pnpm run bots -- status
pnpm run bots -- stop apac              # or no region to stop all
```

The root URL serves a control page with the same actions; its **Mode** picks load bots
or pilots. There is no authentication: anyone with the URL can start and stop bots,
bounded by the 32-bot, 8-pilot and 6-hour limits per region. `stop` stops a region's
load bots and pilots together.

Local development: run both Workers in one session,
`pnpm exec wrangler dev -c bots/wrangler.jsonc -c bots/pilots.wrangler.jsonc` (after
`pnpm run pilot:build`), with `DIRECTORY_URL=http://127.0.0.1:8787` in `bots/.dev.vars`
(ignored by Git; both configs read it) so they find a local server, which lists itself.
The bots Worker alone runs load bots and reports the pilots Worker as unreachable.

## Pilots

With `--brain` (or **Mode: Pilots** on the control page), a region runs up to 8 pilots:
players driven by the bot brain through the browser's own network client
([crates/pilot](../crates/pilot/README.md)), compiled to Wasm
(`bots/generated/sloppy_pilot.wasm`, built by `pnpm run pilot:build`). They run in the
pilots Worker, which `pnpm run pilots:deploy` builds and deploys. Deploy it before the
bots Worker the first time, since the bots Worker binds to it. The pilots see the room
with the region's latency, so they test how the game feels from there: hit
registration, prediction and lag compensation.

```sh
pnpm run pilots:deploy                                        # build the engine and deploy
pnpm run bots -- start wnam 2 --room ABCD2345 --hunt vlad     # hunt vlad in that room
pnpm run bots -- start weur 1 --room ABCD2345 --escort vlad   # fight beside vlad
pnpm run bots -- start apac 3 --brain --host --map harbor     # make a room, play there
pnpm run bots -- start enam 4 --brain                         # join the fullest room with 4 free seats
pnpm run bots -- start wnam 2 --room ABCD2345 --hunt vlad --skill hard --kind heavy --env dev
```

- **Room.** All of a region's pilots play one room: the `--room` code (on the server the
  directory lists it on), a new room (`--host`, on `--map`) whose first pilot creates it
  and hosts it, or else the fullest open standard room with a seat for every pilot.
- **Missions.** `--hunt NAME` takes the other side from NAME and comes after them.
  `--mission hunt` goes after the nearest player on the other team. `--escort NAME` takes
  NAME's side and follows them. Plain `--brain` plays `skirmish`. Pilots switch sides by
  themselves: a team choice between battles, or leave and rejoin during one.
- **Status.** `status` and the control page show each pilot's phase, team, K/D, hits on
  and from players, RTT, view delay, fire-to-flash time, hit offset, prediction
  corrections and phantom hits (shells drawn hitting that the server flew past).
- **Recovery.** A pilot whose connection gave up rejoins after 15 seconds, taking back a
  seat the server still holds. A room that ended or refuses the pilots makes the region
  choose again; one that turns a pilot away while another plays there (a full room) only
  makes that pilot try again later. A Rust panic traps the Wasm instance: the region
  starts a new one and every pilot takes its seat back (`restarts` in the status).
- **Content version.** The engine is game code, stamped with the checkout's content
  version. A server running another version refuses its pilots, and the region reports
  both versions. Deploy the pilots Worker (`pnpm run pilots:deploy`) from the commit the
  target server runs; the load bots have no such tie, and redeploying one Worker leaves
  the other's runs alone. `--env dev` (`environment` in the API, **Servers** on the page)
  plays on the dev server's rooms instead of the Workers' `SERVER_ENVIRONMENT`, for load
  bots and pilots alike.
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
