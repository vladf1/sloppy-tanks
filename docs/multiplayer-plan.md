# Multiplayer v1 plan

Status: proposal, 2026-09-23. Nothing here is implemented yet. Each milestone below is intended to be one pull request.

## Goal

Friends play one Team Battle together over the internet by sharing a room link. A server runs the match; browsers send controls and draw what the server reports. Hosting stays free or close to it, single-player keeps working, and seeded single-player results do not change.

## Decisions

| Decision           | Choice                                                                                                    | Why                                                                                                                                                |
| ------------------ | --------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Who runs the match | The server, one Cloudflare Durable Object per room                                                        | `Simulation` already runs without rendering and ran unmodified in workerd. No host-tab problems; players can't fake damage.                        |
| Transport          | WebSockets, JSON messages in v1                                                                           | The only option on Durable Objects; measured traffic is small. Switch to binary later if needed.                                                   |
| Server address     | `wss://sloppy-tanks-server.<account>.workers.dev`                                                         | No DNS change; fridman.me stays at Namecheap.                                                                                                      |
| Game mode          | Team Battle; players choose team and tank; bots fill empty seats                                          | Respawns and timed rounds need no new rules.                                                                                                       |
| Latency handling   | Other tanks drawn ~100 ms behind and interpolated; own turret aim applied locally; no movement prediction | Keeps v1 small. Prediction is the first follow-up if driving feels laggy.                                                                          |
| Portability        | Room logic in plain TypeScript, separate from the Durable Object API                                      | Keeps a later move to Colyseus or a Node host cheap.                                                                                               |
| Cost               | Workers Free plan; switch to Paid ($5/month) if M1 shows CPU-limit errors                                 | Free allows ~29 match-hours/day of server time and ~18 player-hours/day of input at 30 messages/s. Outgoing messages and bandwidth are not billed. |

Not in v1: own-tank movement prediction, WebTransport, accounts or public matchmaking, co-op Solo Assault, the full per-player battle report (v1 shows a scoreboard), and production rollout.

Considered and set aside:

- **Player-hosted matches:** the host tab pauses when hidden, browsers throttle background tabs, and the match ends when the host leaves.
- **Lockstep or rollback:** needs bit-identical results in every browser. The driving code calls `Math.sin/cos/atan2/hypot` every tick, which engines may round differently, and Rapier's standard build is not cross-platform deterministic.
- **Colyseus:** needs a Node host. Its main extras (prediction, delta-encoded state sync) are not needed for v1.
- **WebTransport:** Cloudflare can't host it; it needs a VM with an open UDP port.

## Measurements behind these decisions

Measured 2026-09-23 in a cloud container (Intel Xeon 2.8 GHz, Node 22). Not measured on player devices or on Cloudflare's production network.

- **Simulation cost:** 12-bot Team Battle, one seed (4242), one 120 s run (7,200 steps) per map. Mean 0.9–1.4 ms per step, p99 2.7–5.4 ms, worst single step 18–21 ms during building collapses.
- **In workerd** (`wrangler dev` 4.136.3): 0.75–1.06 ms per step (one 7,200-step run per map, village twice). A 20 Hz WebSocket loop kept exactly real time with two rooms running.
- **Determinism across builds:** the same seed produced identical scores and body counts in Node (`@dimforge/rapier3d-compat`) and workerd (`@dimforge/rapier3d`) on all three maps. Both are V8; other browser engines were not tested.
- **Memory:** after forced garbage collection, a 3-minute match used ~1–3 MB of JavaScript heap and ~2 MB of Rapier memory. A Durable Object may use 128 MB.
- **Traffic:** a quantized binary snapshot was estimated at 0.4–0.5 KB on average (about 1 KB at most) at 20 Hz, plus 12–17 game events per second. Colyseus's delta encoding of the same match measured 5.4–8.0 KB/s per player including events (one 20 s run per map).

## Architecture

```
Browser (Pages site)                              Cloudflare
┌──────────────────────────────┐   wss   ┌──────────────────────────────────────┐
│ Controls → input, 30/s       │ ──────► │ Worker: /room/CODE → Durable Object  │
│ Mirror Simulation, never     │ ◄────── │ Durable Object "CODE"                │
│ stepped ← snapshots, 20/s    │         │   MatchHost → Simulation at 60 Hz    │
│ Presentation, audio, UI      │         │   seats, inputs, snapshots, events   │
└──────────────────────────────┘         └──────────────────────────────────────┘
```

New modules:

| Path                       | Role                                                                                                                                                         |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/net/protocol.ts`      | Message types and protocol version, shared by client and server                                                                                              |
| `src/net/match-host.ts`    | Room logic without any transport: seats, host controls, applying inputs, building snapshots and full state. Runs in Node tests and inside the Durable Object |
| `src/net/mirror.ts`        | Client: applies full state and snapshots to a `Simulation` copy that is never stepped                                                                        |
| `src/net/interpolation.ts` | Client: snapshot buffer, server-clock estimate, per-entity interpolation                                                                                     |
| `src/net/connection.ts`    | Client: WebSocket, reconnect with a seat token, ping                                                                                                         |
| `server/`                  | Worker entry, Durable Object wrapper, Rapier WASM loader, build script, `wrangler.jsonc`, and a `tsconfig.json` with Workers types                           |

## Protocol v1

JSON messages with a `type` field. The server validates and clamps every client message and closes the connection on malformed or oversized input.

Client → server:

- `join { version, name, kind, team, token? }`. A `token` reclaims a seat after a reconnect.
- `input { seq, moveX, moveZ, aim, fire, mines, ammo? }`, about 30 times a second. `mines` is a running count and `ammo` carries its own sequence number, so the server applies each one-shot action exactly once, even when a message is repeated or superseded. This preserves the rule that one-shot actions are consumed by one simulation tick.
- Host only: `settings { mapMode, difficulty }`, `start`, `end`.
- `ping { t }`.

Server → client:

- `welcome { version, playerId, token, hostId }` and `lobby { players, settings }`.
- `start { seed, round, options, seats }`: enough for the client to build the same map with the same entity ids.
- `full { tick, … }` on start, late join and reconnect: every tank; every cover that changed since the map was built (hp, alive, timber hits, rubble added by collapses); pickups; shots; mines; and fragments with their metadata (shape, size, color, dimensions, wreck part, team).
- `snap { tick, elapsed, match, tanks, shots, mines, bodies, covers?, pickups?, spawns?, events }`, 20 times a second.
  - `tanks` carries position, velocity, heading, aim, hp, shield, recoil, rank, timers, ammo and kills/deaths.
  - `bodies` holds only moving fragments and movable cover.
  - `covers`, `pickups` and `spawns` appear only when something changed.
- `result { scores, winner, players }` and `pong { t, tick }`.
- If the client and server protocol versions differ (for example after a deploy), the client asks the player to reload.

## Milestones

Every milestone ends with `npm run check` passing. Sizes are relative: S, M, L.

### M1: Server skeleton and free-plan check (S)

- Create `server/` with the Rapier loader from Appendix A, a Durable Object running a bot-only Team Battle, and a 20 Hz JSON position stream.
- Add `wrangler`, `esbuild` (currently only a transitive dependency) and `@cloudflare/workers-types` as dev dependencies.
- Add scripts `server:dev` (`wrangler dev`), `server:build` and `server:deploy`.
- Wire the new folder into the tooling:
  - include `server/` in type checking through its own `tsconfig.json`, run from `npm run typecheck`;
  - lint it;
  - ignore `server/dist/` in Git, Prettier and ESLint.
- Deploy to `workers.dev` on the Free plan. Connect 2–4 test clients for 15 minutes and watch `wrangler tail` for CPU-limit errors, evictions or tick drift.
- Needs from the owner: `wrangler login`, or an API token with Workers Scripts:Edit. The existing CI token is scoped to Pages only.
- **Done when:** the server runs 15 minutes without CPU-limit errors, or the decision is made to use the $5 plan.

### M2: Several human players in the simulation, no networking (M)

Single-player behavior and seeded results stay identical.

- **Controls per tank:** add a player seat to `Tank`.
  - `step(command)` keeps working for single-player and delegates to a new `stepWith(commandsByTank)`.
  - A seated tank with no command this tick gets an idle command.
  - A disconnected player's tank is driven by its bot brain, which every tank already has.
- **Separate two meanings of "human":**
  - "A human-controlled tank" drives balance rules.
  - "The local viewer" drives the camera, HUD and audio: introduce a viewer tank id for `presentation.ts`, `ui.ts`, `ui-markup.ts`, `game.ts`, `round-recap.ts` and `touch-controls.ts`.
- **Per-player choices:**
  - Respawn uses the seat's tank kind instead of `simulation.humanKind` (`src/game/tank-lifecycle.ts:140`).
  - `reset()` places players by seat instead of the single `humanTeam` index.
- **Rules that assume one human:**
  - Wreck landing is clamped to the local camera's view (`src/game/wrecks.ts:79`, set in `presentation.ts:603`). Always use the fixed box around the explosion instead. Headless runs already take this branch, so seeded results do not change.
  - Debris cleanup protects pieces near the one human (`src/game/debris-cleanup.ts:21`). Use the nearest human.
  - Solo Assault scores spawns against the one human and ends when that tank dies (`src/game/simulation.ts:475`, `:491`). Use all humans. Easy bots head for the nearest human (`src/game/bot-strategy.ts:173`).
  - Difficulty applies to bots not on `humanTeam` (`src/game/difficulty.ts:37`). With players on both teams, follow the rule chosen under Open questions.
  - Combat records exist for one human. Keep one per tank: `combat-record.ts`, plus its writers in `damage.ts`, `weapons.ts:103`, `pickups.ts:24`, `projectiles.ts:406` and `tank-lifecycle.ts:154`.
  - The kill feed writes "YOU" inside the simulation (`src/game/damage.ts:114`). Build that text in the UI from the event's ids, per viewer.
  - Tank and bullet speed tuning comes from each browser's localStorage and changes shared balance tables (`src/game/speed-tuning.ts`, settings in `src/game.ts`). Make it a match option.
  - Events must hold only plain data. `explode()` spreads its source object into the event, so drum explosions carry a live Rapier body (`src/game/damage.ts:209`, `:233`).
- **Tests:**
  - two humans with different commands move independently;
  - mines and ammo selection apply once per tank;
  - wreck placement ignores `wreckView`;
  - cleanup spares debris near any human;
  - Solo ends only when every human is dead;
  - every event survives `JSON.stringify`.
- **Done when:**
  - `npm run check` passes;
  - `npm run validate` output is unchanged apart from `wallSeconds`, confirmed by inspecting the diff, which is then not committed;
  - `scripts/browser-check.mjs` passes.

### M3: Match host, protocol and headless test harness (M)

- Write `protocol.ts`, `match-host.ts`, `mirror.ts` and `interpolation.ts`.
- **Mirror rules:**
  - The mirror is built from the same seed, options, round and seats. Initial covers, pickups and tanks therefore get the same ids as on the server.
  - Entities created later (shots, mines, fragments, collapse rubble) use the server's ids.
  - The mirror is never stepped; its bodies are only positioned. Fragments get stand-in bodies that answer `translation()`, `rotation()`, `linvel()` and `isSleeping()` for Presentation.
  - `elapsed` follows the server clock, because track dust and wreck aging read it.
- **Harness:** a `MatchHost` and two fake clients connected by an in-memory transport, driven by a fixed-step clock. Each client applies its messages to its own mirror `Simulation`.
- **Tests:**
  - mirror tanks match the server within a tolerance;
  - destroyed cover, collapse rubble and fragments stay in sync;
  - a late join or reconnect restores state from `full`;
  - a dropped player's tank is bot-driven and returns to the player on reconnect;
  - one-shot actions apply once under repeated or dropped input messages;
  - bytes per client per second stay under a budget set from the first measurement.
- **Done when:** the harness runs deterministically in `npm test`.

### M4: Durable Object server (S)

- Wrap `MatchHost` in the Durable Object and route `/room/CODE` to it.
- A timer every ~16 ms runs the steps that are due from an accumulator, usually one; every third step sends a snapshot.
- When the room empties, stop the timer and `dispose()` the simulation.
- **Guardrails:**
  - an `Origin` allowlist: production sites, the dev site, `pages.dev` and localhost;
  - at most 8 players per room;
  - a fixed room-code format;
  - message size and rate limits;
  - the protocol version check.
- **Done when:** two Node test clients complete a round through `wrangler dev`, and again against the deployed `workers.dev` server.

### M5: Client multiplayer mode (L)

- **Menu and lobby:**
  - "Play with friends" creates a room (a random code, with a copy-link button), or the player joins through `?room=CODE`.
  - The lobby lists players and lets each choose a team and tank; the host picks map and difficulty and starts.
  - Display names are stored in localStorage.
- **Network game loop in `game.ts`:**
  - no `sim.step()`;
  - controls are collected into input messages;
  - snapshots are applied through the mirror and interpolated;
  - events go to presentation, audio and UI as they do now.
- **Interpolation:**
  - Write each tank's interpolated pose into both `previous` and the body. Presentation's own interpolation then changes nothing and needs no timing changes, and track marks still work because they compare frame-to-frame positions.
  - The local tank uses the newest snapshot.
  - Shells keep moving along their velocity between snapshots.
  - The local turret follows the pointer immediately.
- **Changes to match flow:**
  - The pause menu no longer pauses the match. END BATTLE is host-only; other players get LEAVE.
  - A hidden tab keeps its connection and jumps to the latest state when shown again.
- **Results:** a scoreboard with each player's kills and deaths.
- **Server URL:** set at build time through `VITE_MULTIPLAYER_URL`, and overridable with `?server=` for local testing against `wrangler dev`.
- **Touch controls** need no changes, because they already produce the same `VehicleCommand`.
- **Browser check:** add `scripts/multiplayer-check.mjs`, which drives two Chrome contexts through create, join, play and results. The existing single-player browser checks must still pass.
- **Done when:** two browsers on one machine play a full round against `wrangler dev`.

### M6: Playtest with friends (S)

- Run the server on `workers.dev` and publish the client to the dev site with `npm run deploy:dev`. The production sites stay unchanged.
- Record ping, bandwidth, CPU per tick, errors (`wrangler tail`) and how laggy it feels.
- **Done when:** several 5-minute rounds with 2–4 players on different networks finish with no players falling out of sync and no dropped rooms, and follow-ups are prioritized.

## After v1

Choose from the playtest results:

- **Own-tank movement prediction:** step the local tank with the same `driveTank` code in the mirror's physics world, correct it when snapshots arrive, and replay inputs the server hasn't confirmed yet.
- **Binary snapshots,** if bandwidth or the free plan's request quota becomes a concern.
- **Per-player battle report,** reusing the existing single-player recap.
- **Co-op Solo Assault.**
- **Production rollout:**
  - point both Pages builds at the server;
  - add a GitHub Actions deploy for the server with a Workers-scoped token.
- **Colyseus,** only if hand-written network code becomes the bottleneck.
- **WebTransport,** only on a host with an open UDP port.

## Risks

| Risk                                                                                  | Mitigation                                                             |
| ------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| The Free plan's 10 ms CPU allowance per incoming message is exceeded during collapses | M1 checks this; the $5 plan raises the limit to 30 s                   |
| Own movement feels laggy: roughly ping plus 50–100 ms                                 | Turret aim is applied locally in v1; prediction is the first follow-up |
| Deploys restart Durable Objects and drop live matches                                 | Deploy when nobody is playing; the version check prompts a reload      |
| A client's mirror drifts from the server                                              | Full state on join and reconnect; the M3 harness asserts sync          |
| The refactor changes seeded single-player results                                     | `npm run validate` output must not change apart from timing            |
| The free request quota (100,000/day, about 18 player-hours at 30 inputs/s) runs out   | Send inputs at 20/s or only on change, or move to the Paid plan        |

## Open questions (proposed defaults)

- **Players per room:** up to 8; bots fill the remaining seats of the 12.
- **Teams:** players choose; the default puts everyone on one team against bots.
- **Late join:** allowed; the newcomer takes a bot's seat.
- **Host leaves:** the host role passes to the next player.
- **Difficulty when players are on both teams:** every bot uses the chosen difficulty.

## Appendix A: Rapier in a Durable Object

Rapier's `@dimforge/rapier3d` loads its WASM with `import * as wasm from "./rapier_wasm3d_bg.wasm"`, which Workers do not support. Workers can import a `.wasm` file as a compiled `WebAssembly.Module` through the `CompiledWasm` rule, so the build swaps the loader for a manual instantiation. The game code imports `@dimforge/rapier3d-compat`, so the build also points that import at `src/game/physics-browser.ts`, as Vite already does for the browser.

```js
// server/build.mjs (excerpt)
import { fileURLToPath } from "node:url";

const repo = (path) => fileURLToPath(new URL(`../${path}`, import.meta.url));

const rapierWorkerd = {
  name: "rapier-workerd",
  setup(context) {
    context.onResolve({ filter: /^@dimforge\/rapier3d-compat$/ }, () => ({
      path: repo("src/game/physics-browser.ts"),
    }));
    // Leave the WASM import for wrangler's CompiledWasm rule.
    context.onResolve({ filter: /^\.\/rapier\.wasm$/ }, () => ({
      path: "./rapier.wasm",
      external: true,
    }));
    context.onLoad({ filter: /rapier3d[\\/]rapier_wasm3d\.js$/ }, () => ({
      loader: "js",
      resolveDir: repo("node_modules/@dimforge/rapier3d"),
      contents: `
        import wasmModule from "./rapier.wasm";
        import * as bg from "./rapier_wasm3d_bg.js";
        const instance = new WebAssembly.Instance(wasmModule, { "./rapier_wasm3d_bg.js": bg });
        bg.__wbg_set_wasm(instance.exports);
        export * from "./rapier_wasm3d_bg.js";
      `,
    }));
  },
};
```

Bundle with `format: "esm"`, `platform: "neutral"`, `mainFields: ["module", "main"]` and `external: ["cloudflare:workers"]`. Then copy `node_modules/@dimforge/rapier3d/rapier_wasm3d_bg.wasm` to `server/dist/rapier.wasm`.

```jsonc
// server/wrangler.jsonc
{
  "name": "sloppy-tanks-server",
  "main": "dist/worker.js",
  "no_bundle": true,
  "compatibility_date": "2026-01-01",
  "rules": [{ "type": "CompiledWasm", "globs": ["**/*.wasm"] }],
  "durable_objects": { "bindings": [{ "name": "ROOM", "class_name": "Room" }] },
  "migrations": [{ "tag": "v1", "new_sqlite_classes": ["Room"] }],
}
```

The test bundle was 1.75 MB of JavaScript plus a 2 MB WASM file, about 1.1 MB gzipped, well under the 64 MiB Worker size limit. The simulation pulls in three.js through three imports: `ai → hitboxes`, `damage → scenery-pieces` and `damage → wrecks → humvee-model`. Separating the data those modules need from the model code would shrink the server bundle, but it is not required.
