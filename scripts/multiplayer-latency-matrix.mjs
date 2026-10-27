/**
 * How a room feels across network conditions: for each condition, two Chrome players
 * join a room with Normal bots, then for `SLOPPY_MATRIX_SECONDS` each drives (often toward
 * the nearest enemy), aims at the nearest enemy as drawn and fires short bursts, behind the
 * dev client's transport delay (`?latency`, `?jitter`, `?stall`). It records both players'
 * Stats for nerds readings (fire to flash, view delay, hit offset, corrections by cause,
 * playout buffer, late batches, snapshot CPU) and the server's wire bytes per player, under
 * `artifacts/performance/multiplayer/latency-matrix/<label>/`.
 *
 * It starts its own server (`target/server/sloppy-server`, `bun run server:build`) on
 * `SLOPPY_MATRIX_PORT` (default 8791) so its byte counts hold only this run; run Vite and
 * pass its URL as `SLOPPY_URL`. `SLOPPY_MATRIX_SERVER=wss://1-2-3-4.nip.io` plays on that
 * remote server over the real network instead (its `/stats` then counts every room it
 * holds), and `SLOPPY_MATRIX_REPEAT` runs each condition that many times. `SLOPPY_MATRIX_PARAMS` adds page parameters to both players
 * (`dodge=0` compares without "dodger first", `shooter=0` without "shooter first"). Manual evidence, not a gate: keep it out of
 * `bun run check`.
 *
 *   SLOPPY_URL=http://127.0.0.1:5174/sloppy-tanks/ SLOPPY_MATRIX_LABEL=baseline \
 *     bun scripts/multiplayer-latency-matrix.mjs
 */
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { setTimeout as wait } from "node:timers/promises";
import {
  chooseMap,
  click,
  collectErrors,
  gameUrl,
  launchChrome,
  localDirectory,
} from "./browser-helpers.mjs";
import { openMultiplayerTab } from "./multiplayer-helpers.mjs";

const PORT = Number(process.env.SLOPPY_MATRIX_PORT ?? 8791);
const LABEL = process.env.SLOPPY_MATRIX_LABEL ?? "run";
const SECONDS = Number(process.env.SLOPPY_MATRIX_SECONDS ?? 60);
const MAP = process.env.SLOPPY_MATRIX_MAP ?? "village";
const PARAMS = new URLSearchParams(process.env.SLOPPY_MATRIX_PARAMS ?? "");
/** Round trip, jitter and stall in milliseconds, as the dev page parameters take them. */
const CONDITIONS = [
  { name: "rtt0", latency: 0, jitter: 0, stall: 0 },
  { name: "rtt50", latency: 50, jitter: 0, stall: 0 },
  { name: "rtt100", latency: 100, jitter: 0, stall: 0 },
  { name: "rtt200", latency: 200, jitter: 0, stall: 0 },
  { name: "rtt100-jitter30", latency: 100, jitter: 30, stall: 0 },
  { name: "rtt100-jitter30-stall200", latency: 100, jitter: 30, stall: 200 },
].filter(
  (condition) =>
    !process.env.SLOPPY_MATRIX_CONDITIONS ||
    process.env.SLOPPY_MATRIX_CONDITIONS.split(",").includes(condition.name),
);
const output = `artifacts/performance/multiplayer/latency-matrix/${LABEL}`;
await mkdir(output, { recursive: true });
const REMOTE = process.env.SLOPPY_MATRIX_SERVER;
const REPEAT = Number(process.env.SLOPPY_MATRIX_REPEAT ?? 1);
// A page reads its server from a directory, and every server lists itself at `/servers`.
const serverHttp = REMOTE ? localDirectory(REMOTE) : `http://127.0.0.1:${PORT}`;

let server;
let serverLog = "";
async function startServer() {
  // Another session's server on this port would answer the health probe in its place.
  const busy = await fetch(`${serverHttp}/health`).then(
    () => true,
    () => false,
  );
  assert.ok(!busy, `port ${PORT} is already in use; set SLOPPY_MATRIX_PORT`);
  server = spawn("target/server/sloppy-server", [], {
    env: { ...process.env, PORT: String(PORT) },
    stdio: ["ignore", "pipe", "pipe"],
  });
  server.stdout.on("data", (data) => (serverLog += data));
  server.stderr.on("data", (data) => (serverLog += data));
  const deadline = Date.now() + 20000;
  for (;;) {
    try {
      if ((await fetch(`${serverHttp}/health`)).ok) return;
    } catch {
      // The child has not bound its HTTP port yet.
    }
    assert.ok(Date.now() < deadline, "the matrix server starts");
    await wait(100);
  }
}

/** The server's cumulative bytes on the wire (after compression) at its latest sample. */
async function serverTotals() {
  const stats = await (await fetch(`${serverHttp}/stats`)).json();
  return {
    sampledAt: Date.parse(stats.sampledAt),
    wireSentMB: stats.totals.wireSentMB,
    wireReceivedMB: stats.totals.wireReceivedMB,
    sentMB: stats.totals.sentMB,
  };
}

function conditionUrl(condition) {
  const url = new URL(gameUrl);
  url.searchParams.set("directory", serverHttp);
  for (const key of ["latency", "jitter", "stall"]) {
    if (condition[key]) url.searchParams.set(key, String(condition[key]));
  }
  for (const [key, value] of PARAMS) url.searchParams.set(key, value);
  return url;
}

async function openPlayer(browser, errors, name) {
  const context = await browser.newContext({ viewport: { width: 1100, height: 780 } });
  const page = await context.newPage();
  // Both players on one machine must keep receiving animation frames.
  await page.addInitScript(() => {
    Object.defineProperty(document, "hidden", { configurable: true, get: () => false });
  });
  collectErrors(page, errors, { consoleErrors: true, prefix: `${name}: ` });
  return page;
}

function playing(page, tanks) {
  return page.waitForFunction(
    (tanks) => {
      const game = window.sloppyMultiplayer;
      return (
        game?.view &&
        game.control?.driver === "human" &&
        game.control.controlEpoch >= 2 &&
        game.display?.match.phase === "playing" &&
        game.display.tanks.length === tanks
      );
    },
    tanks,
    { timeout: 90000 },
  );
}

/** Stats for nerds as the engine reports them. */
const readStats = (page) =>
  page.evaluate(() => JSON.parse(window.sloppyMultiplayer.game.stats_json(performance.now())));

/**
 * Plays for `seconds` through the dev `controls` hook: aims at the nearest live enemy as
 * drawn, fires a short burst every 0.7–1.2 s, and drives in segments of 0.5–1.7 s, most of
 * them toward that enemy. Returns the frames drawn and the presses sent.
 */
function play(page, seconds, seed) {
  return page.evaluate(
    ({ seconds, seed }) =>
      new Promise((resolve) => {
        const room = window.sloppyMultiplayer;
        const { game, controls } = room;
        const canvas = document.querySelector("canvas#game");
        let state = seed >>> 0;
        const random = () => {
          state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
          return state / 4294967296;
        };
        const start = performance.now();
        let frames = 0;
        let presses = 0;
        let segmentEnd = 0;
        let fireUntil = 0;
        let nextFire = 0;
        const toClient = ([x, y]) => {
          const rect = canvas.getBoundingClientRect();
          return [rect.left + ((x + 1) / 2) * rect.width, rect.top + ((1 - y) / 2) * rect.height];
        };
        const step = (now) => {
          if (now - start > seconds * 1000) {
            controls.clear();
            resolve({ frames, presses, seconds: (now - start) / 1000 });
            return;
          }
          requestAnimationFrame(step);
          frames++;
          // The debug scene is serialized JSON; reading it every third frame is plenty.
          if (frames % 3 !== 0) return;
          const display = room.display;
          const viewer = display?.viewer;
          if (!viewer?.alive) return;
          const distance = (tank) =>
            Math.hypot(tank.position.x - viewer.position.x, tank.position.z - viewer.position.z);
          const target = display.tanks
            .filter((tank) => tank.alive && tank.team !== viewer.team)
            .sort((a, b) => distance(a) - distance(b))[0];
          // An enemy off screen is aimed and driven at through a point on the way to it.
          let aim = [];
          if (target) {
            const reach = Math.min(1, 12 / Math.max(distance(target), 1e-6));
            aim = game.debug_screen_point(target.position.x, target.position.z);
            if (aim.length !== 2) {
              aim = game.debug_screen_point(
                viewer.position.x + (target.position.x - viewer.position.x) * reach,
                viewer.position.z + (target.position.z - viewer.position.z) * reach,
              );
            }
          }
          if (aim.length === 2) controls.aimAt(...toClient(aim));
          if (now >= nextFire) {
            fireUntil = now + 150;
            nextFire = now + 700 + random() * 500;
            presses++;
          }
          controls.fire = now < fireUntil;
          if (now >= segmentEnd) {
            segmentEnd = now + 500 + random() * 1200;
            const keys = [];
            const own = game.debug_screen_point(viewer.position.x, viewer.position.z);
            if (aim.length === 2 && own.length === 2 && random() < 0.65) {
              const [dx, dy] = [aim[0] - own[0], aim[1] - own[1]];
              if (dy > 0.08) keys.push("KeyW");
              if (dy < -0.08) keys.push("KeyS");
              if (dx > 0.08) keys.push("KeyD");
              if (dx < -0.08) keys.push("KeyA");
            } else {
              const choices = [
                [],
                ["KeyW"],
                ["KeyS"],
                ["KeyA"],
                ["KeyD"],
                ["KeyW", "KeyA"],
                ["KeyW", "KeyD"],
              ];
              keys.push(...choices[Math.floor(random() * choices.length)]);
            }
            controls.keys.clear();
            for (const key of keys) controls.keys.add(key);
          }
        };
        requestAnimationFrame(step);
      }),
    { seconds, seed },
  );
}

/** The window's own figures from the stats before and after it. */
function derive(before, after, played) {
  const { network, feel } = after;
  const seconds = played.seconds;
  return {
    fps: played.frames / seconds,
    presses: played.presses,
    rttMs: network.rttMs,
    updatesPerSecond: (network.receivedUpdates - before.network.receivedUpdates) / seconds,
    bufferMs: network.bufferMs,
    underrun: network.underrun,
    lateBatches: network.lateBatches - before.network.lateBatches,
    longestBatchGapMs: network.longestBatchGapMs,
    predictionLeadMs: network.predictionLeadMs,
    correctionCmPerS: network.correctionMPerS * 100,
    correctionP95Cm: network.correctionP95M * 100,
    correctionCauses: Object.fromEntries(
      Object.entries(network.correctionCauses).map(([cause, tally]) => [
        cause,
        { count: tally.count, cmPerS: tally.mPerS * 100 },
      ]),
    ),
    inputStarts: network.inputStarts - before.network.inputStarts,
    lateInputStarts: network.lateInputStarts - before.network.lateInputStarts,
    receiveMsPerSecond: (network.receiveMs - before.network.receiveMs) / seconds,
    viewDelayMs: feel.viewDelayMs,
    fireToFlashMs: feel.fireToFlashMs,
    fireToReportMs: feel.fireToReportMs,
    unansweredPresses: feel.unansweredPresses,
    hitOffsetM: feel.hitOffsetM,
    incomingOffsetM: feel.incomingOffsetM,
    incomingBehindM: feel.incomingBehindM,
    unseenIncoming: feel.unseenIncoming,
    phantomStrikes: network.phantomStrikes - before.network.phantomStrikes,
    ownShots: feel.ownShots,
  };
}

async function runCondition(browser, condition, index) {
  const errors = [];
  const url = conditionUrl(condition);
  const alice = await openPlayer(browser, errors, "alice");
  const setup = new URL(url);
  setup.searchParams.set("multiplayer", "");
  await alice.goto(setup.href);
  await openMultiplayerTab(alice);
  await alice.locator("#player-name").fill("Alice");
  await alice.locator('input[name="playerTeam"][value="0"]').check();
  await chooseMap(alice, MAP);
  await alice.locator("#create-humans-only").uncheck();
  await click(alice, "#create-room");
  await alice.waitForFunction(() => window.sloppyMultiplayer?.connection.connected);
  const link = new URL(alice.url());
  for (const [key, value] of url.searchParams) link.searchParams.set(key, value);
  const bob = await openPlayer(browser, errors, "bob");
  await bob.goto(link.href);
  await bob.locator("#join-room:enabled").waitFor();
  await bob.locator("#player-name").fill("Bob");
  await bob.locator('input[name="playerTeam"][value="1"]').check();
  await click(bob, "#join-room");
  const players = [alice, bob];
  await Promise.all(players.map((page) => playing(page, 12)));
  // Let the playout buffer and prediction clock settle before the window opens.
  await wait(3000);
  const before = await Promise.all(players.map(readStats));
  const bytesBefore = await serverTotals();
  const samples = [bytesBefore];
  let playingWindow = true;
  const polling = (async () => {
    while (playingWindow) {
      await wait(2000);
      samples.push(await serverTotals());
    }
  })();
  const played = await Promise.all(
    players.map((page, seat) => play(page, SECONDS, index * 2 + seat + 1)),
  );
  playingWindow = false;
  await polling;
  const after = await Promise.all(players.map(readStats));
  // `/stats` refreshes every ten seconds: the first and last distinct samples in the window.
  const distinct = samples.filter(
    (sample, i) => i === 0 || sample.sampledAt !== samples[i - 1].sampledAt,
  );
  const [first, last] = [distinct[0], distinct.at(-1)];
  const spanSeconds = (last.sampledAt - first.sampledAt) / 1000;
  const bandwidth =
    spanSeconds > 0
      ? {
          spanSeconds,
          wireKBpsPerPlayer: ((last.wireSentMB - first.wireSentMB) * 1024) / spanSeconds / 2,
          rawKBpsPerPlayer: ((last.sentMB - first.sentMB) * 1024) / spanSeconds / 2,
          wireReceivedKBpsPerPlayer:
            ((last.wireReceivedMB - first.wireReceivedMB) * 1024) / spanSeconds / 2,
        }
      : null;
  await players[0].screenshot({ path: `${output}/${condition.name}-alice.png` });
  const result = {
    condition,
    map: MAP,
    seconds: SECONDS,
    bandwidth,
    players: players.map((_, seat) => derive(before[seat], after[seat], played[seat])),
    raw: { before, after, played, serverSamples: distinct },
    errors,
  };
  await writeFile(`${output}/${condition.name}.json`, JSON.stringify(result, null, 2));
  for (const page of players) await page.context().close();
  return result;
}

const round = (value, digits = 0) =>
  Number.isFinite(value) ? Number(value.toFixed(digits)) : null;
/** One row per player of a condition, for the console. */
function rows(result) {
  return result.players.map((player, seat) => ({
    condition: result.condition.name,
    player: seat === 0 ? "alice" : "bob",
    fps: round(player.fps),
    "upd/s": round(player.updatesPerSecond, 1),
    "flash ms": `${round(player.fireToFlashMs.mean)} p95 ${round(player.fireToFlashMs.p95)} (${player.fireToFlashMs.count})`,
    "report ms": round(player.fireToReportMs.mean),
    "view ms": round(player.viewDelayMs.mean),
    "buffer ms": round(player.bufferMs),
    "hit m": `${round(player.hitOffsetM.mean, 2)} p95 ${round(player.hitOffsetM.p95, 2)} (${player.hitOffsetM.count})`,
    "incoming m": `${round(player.incomingOffsetM.mean, 2)} p95 ${round(player.incomingOffsetM.p95, 2)} (${player.incomingOffsetM.count}) ${player.unseenIncoming} unseen ${player.phantomStrikes} phantom`,
    "behind m": `${round(player.incomingBehindM.mean, 2)} p95 ${round(player.incomingBehindM.p95, 2)} (${player.incomingBehindM.count})`,
    "corr cm/s": round(player.correctionCmPerS, 1),
    "late batches": player.lateBatches,
    "late inputs": `${player.lateInputStarts}/${player.inputStarts}`,
    "rx ms/s": round(player.receiveMsPerSecond, 1),
    "own shots": player.ownShots
      ? `${player.ownShots.confirmed}/${player.ownShots.predicted} -${player.ownShots.unconfirmedShells} ${player.ownShots.phantomStrikes ?? 0} phantom`
      : null,
    "KB/s": round(result.bandwidth?.wireKBpsPerPlayer, 1),
  }));
}

const browser = await launchChrome();
const results = [];
try {
  if (!REMOTE) await startServer();
  const runs = CONDITIONS.flatMap((condition) =>
    Array.from({ length: REPEAT }, (_, repeat) => ({
      ...condition,
      name: REPEAT > 1 ? `${condition.name}-${repeat + 1}` : condition.name,
    })),
  );
  for (const [index, condition] of runs.entries()) {
    console.log(`${condition.name}: ${SECONDS} s on ${MAP}…`);
    const result = await runCondition(browser, condition, index);
    results.push(result);
    console.table(rows(result));
    if (result.errors.length) console.log("errors:", result.errors);
  }
} finally {
  await browser.close();
  server?.kill("SIGTERM");
  await writeFile(`${output}/server.log`, serverLog);
  await writeFile(
    `${output}/summary.json`,
    JSON.stringify(
      results.map(({ condition, bandwidth, players }) => ({ condition, bandwidth, players })),
      null,
      2,
    ),
  );
}
console.table(results.flatMap(rows));
