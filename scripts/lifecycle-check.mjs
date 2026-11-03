// The page's engines handing the window back and forth: on one page, a single-player
// round, then a room (W held while leaving it), five times over, then a last round.
// After each leave the room's sockets are closed, and over the cycles the page's
// listeners, intervals, WebGPU devices and Wasm memory stop growing; no round starts
// with the tank driving on a key held in the room before, and no renderer or page error
// shows. Then a hidden page (its visibility emulated, as switching tabs reports it):
// single player pauses, and a room keeps its seat on the server for 20 seconds.
//
// The check starts and stops its own server (`SLOPPY_SERVER_BIN`, default the build of
// `bun scripts/build-server.mjs`) on a free port; SLOPPY_WEBGL=1 plays it on WebGL2.
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { setTimeout as wait } from "node:timers/promises";
import {
  clickWidget,
  collectErrors,
  findWidget,
  installProbes,
  launchChrome,
  nextFrame,
  openShell,
  roomListing,
  startGameServer,
  untilState,
} from "./browser-helpers.mjs";

const out = "artifacts/performance/lifecycle-check";
mkdirSync(out, { recursive: true });
/** Single player → room → leave, this many times on one page. */
const CYCLES = 5;
/** A room's arena compiles before its battle shows; a cold start takes a while. */
const ROOM_TIMEOUT_MS = 90_000;
/** How far a parked tank may settle in a round's first seconds. */
const STILL_M = 0.3;
/** How far W must drive the tank in the room. */
const DRIVEN_M = 0.5;
/** How long the room's page stays hidden. */
const HIDDEN_MS = 20_000;
/** Wasm memory the later cycles may still add to the second's, bytes: the page keeps its
 * peak (linear memory never shrinks), so a leak grows it cycle after cycle. */
const MEMORY_GROWTH_BYTES = 32 * 1024 * 1024;

const server = await startGameServer();
const browser = await launchChrome();
const errors = [];
const cycles = [];
const observations = {};
const distance = (a, b) => Math.hypot(a.x - b.x, a.z - b.z);

const context = await browser.newContext({
  viewport: { width: 1280, height: 800 },
  deviceScaleFactor: 1,
});
await installProbes(context);
// GPU objects alive, by kind: made and neither destroyed (deleted) nor garbage collected.
// Each engine's renderer requests its own WebGPU device and the egui overlay its own GL
// objects; an engine that gave the window up must leave none of them behind.
await context.addInitScript(() => {
  const live = (window.gpuObjects = {});
  const tokens = new WeakMap();
  const free = (token) => {
    if (token && !token.freed) {
      token.freed = true;
      live[token.kind] -= 1;
    }
  };
  const collected = new FinalizationRegistry(free);
  const made = (kind, object) => {
    if (object) {
      const token = { kind, freed: false };
      tokens.set(object, token);
      collected.register(object, token);
      live[kind] = (live[kind] ?? 0) + 1;
    }
    return object;
  };
  /** Count what `prototype[create]` makes and `release` frees as `kind`. */
  const watch = (prototype, create, kind, [releaser, release] = []) => {
    const original = prototype?.[create];
    if (!original) return;
    prototype[create] = function (...args) {
      const result = original.apply(this, args);
      return result instanceof Promise
        ? result.then((object) => made(kind, object))
        : made(kind, result);
    };
    const freeing = releaser?.[release];
    if (!freeing) return;
    releaser[release] = function (...args) {
      // A WebGL context deletes its argument; a WebGPU object destroys itself.
      free(tokens.get(releaser === prototype ? args[0] : this));
      return freeing.apply(this, args);
    };
  };
  watch(globalThis.GPUAdapter?.prototype, "requestDevice", "GPUDevice", [
    globalThis.GPUDevice?.prototype,
    "destroy",
  ]);
  watch(globalThis.GPUDevice?.prototype, "createBuffer", "GPUBuffer", [
    globalThis.GPUBuffer?.prototype,
    "destroy",
  ]);
  watch(globalThis.GPUDevice?.prototype, "createTexture", "GPUTexture", [
    globalThis.GPUTexture?.prototype,
    "destroy",
  ]);
  const gl = globalThis.WebGL2RenderingContext?.prototype;
  for (const kind of [
    "Buffer",
    "Texture",
    "Framebuffer",
    "Renderbuffer",
    "VertexArray",
    "Program",
    "Shader",
  ]) {
    watch(gl, `create${kind}`, `GL ${kind}`, [gl, `delete${kind}`]);
  }
});
const page = await context.newPage();
const cdp = await context.newCDPSession(page);
collectErrors(page, errors, { consoleErrors: true });
const shot = (name) => page.screenshot({ path: `${out}/${name}.png` });
const openSockets = () => page.evaluate(() => window.sloppyProbes.openSockets());
// Room list requests while single player has the page: its tab never polls them.
let singlePlayer = false;
observations.singlePlayerListRequests = 0;
page.on("request", (request) => {
  if (singlePlayer && new URL(request.url()).pathname === "/servers") {
    observations.singlePlayerListRequests += 1;
  }
});

/** Battle Setup's single-player tab with its arena ready. */
async function setupReady() {
  if ((await nextFrame(page)).multiplayerTab) await clickWidget(page, "SINGLE PLAYER");
  await untilState(
    page,
    (state) => state.engine === "single" && !state.inRound && !state.multiplayerTab && state.ready,
    { timeout: ROOM_TIMEOUT_MS, what: "Battle Setup's arena ready" },
  );
  singlePlayer = true;
}

/** A single-player round: the tank stays put with no key down (W, held since the room,
 * reached no engine of this round), then the battle report and Battle Setup again. */
async function singleRound(cycle, { hide = false } = {}) {
  await setupReady();
  await clickWidget(page, "GO!");
  let state = await untilState(page, (state) => state.inRound && state.phase === "playing", {
    extras: ["debug"],
    what: `cycle ${cycle}: the round`,
  });
  const start = state.extras.debug.human;
  await wait(1500);
  state = await nextFrame(page, ["debug"]);
  const crept = distance(start, state.extras.debug.human);
  assert.ok(crept < STILL_M, `cycle ${cycle}: the tank drove by itself, ${crept.toFixed(2)} m`);
  await page.keyboard.up("KeyW");
  if (hide) {
    // A hidden page pauses the round, as switching tabs did on the old page.
    await page.evaluate(() => window.sloppyProbes.setHidden(true));
    await wait(1000);
    await page.evaluate(() => window.sloppyProbes.setHidden(false));
    await untilState(page, (state) => state.phase === "paused", { what: "the hidden pause" });
    await shot("hidden-paused");
    await clickWidget(page, "RESUME");
    await untilState(page, (state) => state.phase === "playing", { what: "the resume" });
  }
  await page.keyboard.press("Escape");
  await clickWidget(page, "END BATTLE");
  await untilState(page, (state) => state.phase === "results", { what: "the battle report" });
  await clickWidget(page, "BATTLE SETUP");
  await untilState(page, (state) => !state.inRound, { what: "Battle Setup" });
}

/** CREATE ROOM on the Multiplayer tab, until its battle shows. */
async function createRoom(cycle) {
  singlePlayer = false;
  await clickWidget(page, "MULTIPLAYER");
  await untilState(page, async () => (await findWidget(page, "CREATE ROOM"))?.enabled, {
    what: `cycle ${cycle}: CREATE ROOM`,
  });
  await clickWidget(page, "CREATE ROOM");
  return untilState(
    page,
    (state) =>
      state.engine === "room" &&
      !state.room.joining &&
      state.room.link === "Live" &&
      state.room.phase === "Playing",
    { extras: ["debug"], timeout: ROOM_TIMEOUT_MS, what: `cycle ${cycle}: the room's battle` },
  );
}

/** A room's battle, driven with W, left with W still held. */
async function roomRound(cycle) {
  let state = await createRoom(cycle);
  assert.equal(await openSockets(), 1, `cycle ${cycle}: one socket for the room`);
  const start = state.extras.debug.display.viewer.position;
  await page.keyboard.down("KeyW");
  await wait(1000);
  state = await nextFrame(page, ["debug"]);
  const driven = distance(start, state.extras.debug.display.viewer.position);
  assert.ok(driven > DRIVEN_M, `cycle ${cycle}: W drives in the room, ${driven.toFixed(2)} m`);
  await page.keyboard.press("Escape");
  await clickWidget(page, "LEAVE ROOM");
  await untilState(page, (state) => state.engine === "single" && state.multiplayerTab, {
    what: `cycle ${cycle}: Battle Setup after leaving`,
  });
  await untilState(page, async () => (await openSockets()) === 0, {
    timeout: 10000,
    what: `cycle ${cycle}: the room's socket closed`,
  });
}

/** What the page holds now, after a full garbage collection. */
async function measure(cycle) {
  // Finalizers run in a later task than the collection that found their objects.
  for (let pass = 0; pass < 2; pass += 1) {
    await cdp.send("HeapProfiler.collectGarbage");
    await wait(300);
  }
  const state = await nextFrame(page);
  const probes = await page.evaluate(() => ({
    wasmBytes: window.sloppyProbes.wasmBytes(),
    listeners: window.sloppyProbes.listenerCounts(),
    intervals: window.sloppyProbes.intervalCount(),
    socketsCreated: window.sloppyProbes.socketsCreated,
    openSockets: window.sloppyProbes.openSockets(),
    gpuObjects: { ...window.gpuObjects },
    jsHeap: performance.memory?.usedJSHeapSize ?? null,
  }));
  return { cycle, error: state.error, ...probes };
}

const total = (listeners) => Object.values(listeners).reduce((sum, count) => sum + count, 0);
const mb = (bytes) => Math.round((bytes / 1024 / 1024) * 10) / 10;

try {
  await openShell(page, `directory=${server.directory}`);
  for (let cycle = 1; cycle <= CYCLES; cycle += 1) {
    await singleRound(cycle);
    await roomRound(cycle);
    const measured = await measure(cycle);
    cycles.push(measured);
    console.log(
      `cycle ${cycle}: wasm ${measured.wasmBytes.map(mb).join("+")} MB, ` +
        `${total(measured.listeners)} listeners, ${measured.intervals} intervals, ` +
        `${measured.socketsCreated} sockets made, GPU objects alive ` +
        JSON.stringify(measured.gpuObjects),
    );
    assert.equal(measured.error, null, `cycle ${cycle}: renderer errors`);
  }
  await shot("after-cycles");
  // The last leave's W reached no round either; a hidden page pauses this one.
  await singleRound(CYCLES + 1, { hide: true });

  // Nothing grows from the second cycle on (the first builds what the page keeps).
  const [second, last] = [cycles[1], cycles.at(-1)];
  const memory = (measured) => measured.wasmBytes.reduce((sum, bytes) => sum + bytes, 0);
  observations.memoryGrowth = memory(last) - memory(second);
  assert.ok(
    observations.memoryGrowth <= MEMORY_GROWTH_BYTES,
    `Wasm memory grew ${mb(observations.memoryGrowth)} MB from cycle 2 to ${CYCLES}`,
  );
  const grown = Object.entries(last.listeners).filter(
    ([name, count]) => count > (second.listeners[name] ?? 0),
  );
  assert.deepEqual(grown, [], "listeners added each cycle and never removed");
  assert.ok(
    last.intervals <= second.intervals,
    `intervals: ${second.intervals} → ${last.intervals}`,
  );
  // A leak leaves at least one object a cycle; when the collector last ran moves a count
  // by one either way (187 or 188 buffers alike), so a smaller rise is not one.
  const leftAlive = Object.entries(last.gpuObjects).filter(
    ([kind, count]) => count - (second.gpuObjects[kind] ?? 0) >= CYCLES - 2,
  );
  assert.deepEqual(leftAlive, [], "GPU objects left alive by each cycle");
  assert.equal(observations.singlePlayerListRequests, 0, "single player polls no room list");

  // A hidden room page keeps its seat: the server lists the player throughout.
  let state = await createRoom("hidden");
  const room = state.room.code;
  const player = state.extras.debug.connection.playerId;
  const framesBefore = await page.evaluate(() => window.sloppyShell.state().frame);
  await page.evaluate(() => window.sloppyProbes.setHidden(true));
  const listings = [];
  const hiddenStart = Date.now();
  while (Date.now() - hiddenStart < HIDDEN_MS) {
    const listing = await roomListing(server.directory, room);
    listings.push(listing ? `${listing.players}/${listing.reserved}` : "gone");
    assert.equal(listing?.players, 1, `the hidden player stays connected: ${listings.join(" ")}`);
    await wait(1000);
  }
  observations.hiddenFrames =
    (await page.evaluate(() => window.sloppyShell.state().frame)) - framesBefore;
  await page.evaluate(() => window.sloppyProbes.setHidden(false));
  // Shown again, the player drives its own tank again.
  state = await untilState(
    page,
    (state) =>
      state.engine === "room" &&
      state.room.link === "Live" &&
      state.room.players === 1 &&
      state.extras.debug.control.driver === "human",
    { extras: ["debug"], what: "the room shown again" },
  );
  assert.equal(state.extras.debug.connection.playerId, player, "the hidden page kept its seat");
  observations.hiddenListings = listings;
  await shot("hidden-room-back");
  assert.equal(state.error, null, "renderer errors after the hidden room");

  assert.deepEqual(errors, [], "page and console errors");
  console.log(
    JSON.stringify({
      memoryMB: cycles.map((measured) => measured.wasmBytes.map(mb)),
      memoryGrowthMB: mb(observations.memoryGrowth),
      listeners: cycles.map((measured) => total(measured.listeners)),
      intervals: cycles.map((measured) => measured.intervals),
      hiddenFrames: observations.hiddenFrames,
    }),
  );
  console.log(
    `lifecycle-check: ${CYCLES} single player → room → leave cycles with no leaked sockets, ` +
      "listeners, devices or memory growth, no stuck key, a hidden pause and a hidden " +
      "room's seat passed",
  );
} finally {
  writeFileSync(`${out}/result.json`, JSON.stringify({ cycles, observations, errors }, null, 2));
  writeFileSync(`${out}/server.log`, server.logs());
  await browser.close();
  await server.stop();
}
