# Traffic bots

Network-load players for the multiplayer server. A bot joins through the public
room protocol like a browser tab but never reads the map: it drives in random
directions, sweeps its turret, fires, and sometimes drops mines or switches
ammunition. Bots are named `bot-<region>-<n>` and occupy real seats.

The `sloppy-tanks-bots` Worker (`https://sloppy-tanks-bots.vova145.workers.dev`)
targets `SERVER_URL` in `wrangler.jsonc`: the self-hosted game server on the VPS,
`https://45-63-56-58.sslip.io`. It is separate from the game server and both Pages
sites, and still runs on Cloudflare so bots can connect from many regions.

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

Every run has a deadline (default 30 minutes, maximum 6 hours). A 30-second
alarm stops the run at the deadline and brings the bots back if a deploy or
runtime restart evicted the object. Stopping sends `leave`, which frees seats
immediately.

## Use

```sh
npm run bots:deploy                    # deploy after changing bot code
npm run bots -- start weur 4 --host    # --minutes 30 --room CODE --per-room 1
npm run bots -- start apac 2           # join whatever rooms are open
npm run bots -- status
npm run bots -- stop apac              # or no region to stop all
```

The root URL serves a control page with the same actions. There is no
authentication: anyone with the URL can start and stop bots, bounded by the
32-bot and 6-hour limits per region. Local
development: `npx wrangler dev --config bots/wrangler.jsonc` with `SERVER_URL`
overridden to a local server.

## Cost

Nothing runs between runs: no cron, and a stopped object has no alarm. While a
region runs, its object stays awake and bills duration (128 MB x wall time)
because its outbound sockets cannot hibernate. That's about 460 GB-s per region-hour.
Check the account's plan allowances before long or wide runs; current limits are
on the
[Durable Objects pricing page](https://developers.cloudflare.com/durable-objects/platform/pricing/).
On the game server, a bot costs the same as a human player: about 20 inputs a
second in, and the room's full snapshot stream out (roughly 55–110 KB/s per bot,
which counts against the VPS's monthly transfer).

`/api/status` queries all nine region objects, and the control page polls it
every 10 seconds while visible. Close the page when you are not watching.
