// Two players in a room on a local game server, each in its own headless Chrome context,
// through the page's canvas: one creates the room in a browser without Pointer Lock, as
// iPadOS Safari is (its address names it, its seat waits in sessionStorage), copies the
// invite link and the other joins by it; both drive and see each other move, and the
// battle polls no room list. The development delay slows the round trips; a socket drop
// shows the reconnect dialog and recovers the seat. The second leaves for Battle Setup's
// Multiplayer tab, comes back by typing the code, and a reload takes its reserved seat
// again rather than a third. The host ends the battle and plays again. A server crash
// shows CONNECTION LOST and reconnects to the restarted server, and a graceful stop
// shows ROOM CLOSED with the way back to Battle Setup. A directory that lists a server of
// another content version shows its room as not joinable and offers no new room.
//
// The check starts and stops its own server (`SLOPPY_SERVER_BIN`, default the build of
// `bun scripts/build-server.mjs`) on a free port; SLOPPY_WEBGL=1 plays it on WebGL2.
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import http from "node:http";
import { setTimeout as wait } from "node:timers/promises";
import {
  DEAD_SOCKET,
  clickWidget,
  findWidget,
  freePort,
  gameUrl,
  installProbes,
  launchChrome,
  nextFrame,
  openAddress,
  openShell,
  roomListing,
  shellDebug,
  startGameServer,
  untilState,
} from "./browser-helpers.mjs";

const out = "artifacts/performance/multiplayer-check";
mkdirSync(out, { recursive: true });
/** A room's arena compiles before its battle shows; a cold start takes a while. */
const ROOM_TIMEOUT_MS = 90_000;
/** How long the staged network drop refuses new sockets. */
const DROP_MS = 4000;
/** How far a tank must have driven in the other player's view. */
const DRIVEN_M = 0.5;
/** The client refreshes unchanged input about once a second; this window sees 5-8. */
const IDLE_WINDOW_MS = 6500;
/** How soon a changed input must go out: a frame or two, and the check's own round trip. */
const PROMPT_MS = 200;
/** Console errors a staged outage causes: refused sockets and directory polls. */
const OUTAGE_NOISE = [/WebSocket connection to .* failed/, /ERR_CONNECTION_REFUSED/];

const server = await startGameServer();
const browser = await launchChrome();
const errors = [];
/** Console lines the staged outages caused, kept for the record rather than failed on. */
const noise = [];
let outage = false;
const observations = {};
const distance = (a, b) => Math.hypot(a.x - b.x, a.z - b.z);
const query = (page) => new URL(page.url()).searchParams;

/**
 * A player in a fresh context, its errors collected under `name`. Without `pointerLock`
 * the browser has no Pointer Lock API at all, as iPadOS Safari has none.
 */
async function player(name, { pointerLock = true } = {}) {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
  });
  await context.grantPermissions(["clipboard-read", "clipboard-write"], {
    origin: new URL(gameUrl).origin,
  });
  await installProbes(context);
  if (!pointerLock) {
    await context.addInitScript(() => {
      delete Element.prototype.requestPointerLock;
      delete Document.prototype.exitPointerLock;
      delete Document.prototype.pointerLockElement;
    });
  }
  const page = await context.newPage();
  page.on("pageerror", (error) => errors.push(`${name}: ${error.message}`));
  page.on("console", (message) => {
    if (message.type() !== "error") return;
    const text = message.text();
    const expected =
      text.includes(DEAD_SOCKET) || (outage && OUTAGE_NOISE.some((pattern) => pattern.test(text)));
    (expected ? noise : errors).push(`${name}: ${text}`);
  });
  page.shot = (shot) => page.screenshot({ path: `${out}/${name}-${shot}.png` });
  // Room list requests: Battle Setup's Multiplayer tab polls, a battle must not.
  page.listRequests = 0;
  page.on("request", (request) => {
    if (new URL(request.url()).pathname === "/servers") page.listRequests += 1;
  });
  return page;
}

/** The room's battle, with the room's own socket live and its arena shown. */
function inRoom(page, test = () => true, what = "the room's battle") {
  return untilState(
    page,
    (state) =>
      state.engine === "room" &&
      !state.room.joining &&
      state.room.link === "Live" &&
      state.room.phase === "Playing" &&
      test(state),
    { extras: ["debug"], timeout: ROOM_TIMEOUT_MS, what },
  );
}

/** Battle Setup's Multiplayer tab once its list shows `room` chosen. */
function roomChosen(page, room) {
  return untilState(
    page,
    async (state) =>
      state.engine === "single" && state.multiplayerTab && (await findWidget(page, room))?.selected,
    { what: `Battle Setup with room ${room} chosen` },
  );
}

/** Leave the room from its menu, for Battle Setup's Multiplayer tab. */
async function leave(page) {
  await page.keyboard.press("Escape");
  await clickWidget(page, "LEAVE ROOM");
  await untilState(page, (state) => state.engine === "single" && state.multiplayerTab, {
    what: "Battle Setup after leaving",
  });
}

const tank = (state, id) => state.extras.debug.display.tanks.find((tank) => tank.id === id);
const seatKey = (room) => `sloppy-seat:${server.url}:${room}`;
const seat = (page, room) =>
  page.evaluate((key) => JSON.parse(sessionStorage.getItem(key) ?? "null"), seatKey(room));
const openSockets = (page) => page.evaluate(() => window.sloppyProbes.openSockets());
const pageTime = (page) => page.evaluate(() => performance.now());
/** The room `input` messages `page` sent from `since` (its own clock) on. */
const inputsSince = (page, since) =>
  page.evaluate((since) => window.sloppyProbes.inputs.filter((input) => input.at >= since), since);

/** Until the server lists `room` as `test` wants it. */
async function listed(room, test, what) {
  const start = Date.now();
  for (;;) {
    const listing = await roomListing(server.directory, room).catch(() => undefined);
    if (test(listing)) return listing;
    assert.ok(Date.now() - start < 20000, `${what}: ${JSON.stringify(listing)}`);
    await wait(250);
  }
}

/** Neither page reports a renderer error. */
async function noRendererErrors(pages, when) {
  for (const page of pages) {
    assert.equal((await nextFrame(page)).error, null, `renderer errors ${when}`);
  }
}

try {
  // A creates a humans-only room on the Blue team; its battle starts at once.
  const a = await player("a", { pointerLock: false });
  await openShell(a, `directory=${server.directory}&multiplayer`);
  await untilState(a, (state) => state.multiplayerTab, { what: "the Multiplayer tab" });
  await clickWidget(a, "Blue");
  await clickWidget(a, "CREATE ROOM");
  let aState = await inRoom(a);
  const room = aState.room.code;
  assert.match(room, /^[A-Z0-9]{8}$/);
  assert.equal(query(a).get("room"), room, "the address names the room");
  assert.equal(query(a).has("multiplayer"), false, "the room replaces the tab in the address");
  const aSeat = await seat(a, room);
  assert.ok(
    aSeat?.token && aSeat.roomEpoch,
    `A's seat in sessionStorage: ${JSON.stringify(aSeat)}`,
  );
  assert.equal(aState.room.players, 1);
  assert.equal(await openSockets(a), 1, "one socket for the room");
  const rows = (await nextFrame(a, ["nerd"])).extras.nerd.flatMap((section) => section.rows ?? []);
  observations.graphics = rows.find((row) => row.label === "Graphics API")?.value;
  assert.equal(observations.graphics, process.env.SLOPPY_WEBGL ? "WebGL" : "WebGPU");
  const aPolls = a.listRequests;
  await a.shot("created");

  // The room menu copies the page's own address as the invite link; the rules stay
  // locked during the battle.
  await a.keyboard.press("Escape");
  await untilState(a, async (state) => state.room.menuOpen && (await findWidget(a, "RESUME")), {
    what: "the room menu",
  });
  assert.equal(await findWidget(a, "Change rules"), undefined, "the rules lock during play");
  await clickWidget(a, "Copy invite link");
  await untilState(a, async () => Boolean(await findWidget(a, "Link copied")), {
    what: "Link copied",
  });
  const invite = await a.evaluate(() => navigator.clipboard.readText());
  assert.equal(invite, a.url(), "the invite link is the room's address");
  await a.shot("menu");
  await clickWidget(a, "RESUME");
  await inRoom(a, (state) => !state.room.menuOpen);

  // B opens the invite link, its code in lower case: Battle Setup with the room chosen.
  const b = await player("b");
  await openAddress(b, invite.replace(room, room.toLowerCase()));
  await roomChosen(b, room);
  assert.ok(await findWidget(b, "JOIN ROOM"), "JOIN ROOM is offered for the linked room");
  await b.shot("invite-opened");
  await clickWidget(b, "Red");
  await clickWidget(b, "JOIN ROOM");
  let bState = await inRoom(b, (state) => state.room.players === 2, "B in the room with A");
  aState = await inRoom(a, (state) => state.room.players === 2, "A sees B join");
  assert.equal(query(b).get("room"), room, "B's address names the room");
  const aTank = aState.extras.debug.control.tankId;
  const bTank = bState.extras.debug.control.tankId;
  assert.notEqual(aTank, bTank);
  assert.equal(tank(aState, aTank).team, 0, "A plays Blue");
  assert.equal(tank(aState, bTank).team, 1, "B plays Red");

  // Unchanged input is refreshed about once a second, and the idle player keeps its tank.
  await wait(2000);
  const idleControl = (await nextFrame(a, ["debug"])).extras.debug.control;
  let since = await pageTime(a);
  await wait(IDLE_WINDOW_MS);
  observations.idleInputs = (await inputsSince(a, since)).length;
  assert.ok(
    observations.idleInputs >= 5 && observations.idleInputs <= 8,
    `unchanged input about once a second: ${observations.idleInputs} in ${IDLE_WINDOW_MS} ms`,
  );
  const control = (await nextFrame(a, ["debug"])).extras.debug.control;
  assert.equal(control.driver, "human", "the idle player keeps its tank");
  assert.equal(control.controlEpoch, idleControl.controlEpoch, "no takeover while idle");

  // Both drive; each sees the other's tank move. Held keys keep input coming, and the
  // release goes out at once.
  aState = await nextFrame(a, ["debug"]);
  bState = await nextFrame(b, ["debug"]);
  const before = { a: aState, b: bState };
  since = await pageTime(a);
  await a.keyboard.down("KeyW");
  await b.keyboard.down("KeyW");
  await wait(1500);
  const released = await pageTime(a);
  await a.keyboard.up("KeyW");
  await b.keyboard.up("KeyW");
  await wait(500);
  const driving = await inputsSince(a, since);
  observations.drivingInputs = driving.filter((input) => input.at < released).length;
  assert.ok(observations.drivingInputs >= 8, `held W sends input: ${observations.drivingInputs}`);
  const neutral = driving.find(
    (input) => input.at >= released && input.moveX === 0 && input.moveZ === 0,
  );
  observations.releaseMs = neutral && Math.round(neutral.at - released);
  assert.ok(
    neutral && neutral.at - released < PROMPT_MS,
    `W's release sent: ${observations.releaseMs} ms`,
  );
  aState = await nextFrame(a, ["debug"]);
  bState = await nextFrame(b, ["debug"]);
  observations.driven = {
    aSeenByB: distance(tank(before.b, aTank).position, tank(bState, aTank).position),
    bSeenByA: distance(tank(before.a, bTank).position, tank(aState, bTank).position),
  };
  assert.ok(
    observations.driven.aSeenByB > DRIVEN_M,
    `B sees A drive: ${observations.driven.aSeenByB}`,
  );
  assert.ok(
    observations.driven.bSeenByA > DRIVEN_M,
    `A sees B drive: ${observations.driven.bSeenByA}`,
  );
  await a.shot("driven");
  await b.shot("driven");

  // Aim wakes input; held fire is renewed and its release sent; a mine click goes once.
  since = await pageTime(a);
  await a.mouse.move(900, 300);
  await wait(PROMPT_MS);
  assert.ok((await inputsSince(a, since)).length > 0, "an aim change sends input");
  since = await pageTime(a);
  await a.mouse.down();
  await wait(1500);
  await a.mouse.up();
  await wait(PROMPT_MS);
  const firing = await inputsSince(a, since);
  observations.firingInputs = firing.filter((input) => input.fire).length;
  assert.ok(observations.firingInputs >= 5, `held fire renewed: ${observations.firingInputs}`);
  assert.equal(firing.at(-1).fire, false, "the fire button's release sent");
  since = await pageTime(a);
  await a.mouse.click(900, 300, { button: "right" });
  await wait(1000);
  const mines = (await inputsSince(a, since)).reduce((sum, input) => sum + input.mines, 0);
  assert.equal(mines, 1, "a mine click is sent once");

  // The development delay (`?latency`, a round trip's milliseconds) reaches B's pings.
  await shellDebug(b, "latency", 200, 0, 0);
  bState = await untilState(b, (state) => state.extras.debug.connection.rtt > 180, {
    extras: ["debug"],
    timeout: 20000,
    what: "a slow round trip",
  });
  observations.delayedRtt = bState.extras.debug.connection.rtt;
  await shellDebug(b, "latency", 0, 0, 0);
  bState = await untilState(b, (state) => state.extras.debug.connection.rtt < 100, {
    extras: ["debug"],
    timeout: 20000,
    what: "the round trip back to normal",
  });

  // A network drop: CONNECTION LOST while the client retries, then the same seat.
  const bPlayer = bState.extras.debug.connection.playerId;
  await b.evaluate((ms) => window.sloppyProbes.dropSockets(ms), DROP_MS);
  await untilState(b, (state) => /CONNECTION LOST/.test(state.room.link), {
    what: "the reconnect dialog",
  });
  await untilState(b, () => findWidget(b, "LEAVE ROOM"), {
    timeout: 5000,
    what: "the reconnect dialog's LEAVE ROOM",
  });
  await b.shot("reconnecting");
  bState = await inRoom(b, () => true, "B back after the drop");
  assert.equal(bState.extras.debug.connection.playerId, bPlayer, "the drop keeps B's seat");
  await inRoom(a, (state) => state.room.players === 2, "A sees B back");

  assert.equal(a.listRequests, aPolls, "no room list requests during A's battle");

  // B leaves on purpose: the Multiplayer tab, the room gone from the address, no socket.
  await leave(b);
  assert.equal(query(b).has("room"), false, "leaving drops the room from the address");
  assert.equal(query(b).has("multiplayer"), true, "leaving keeps the Multiplayer tab");
  await untilState(b, async () => (await openSockets(b)) === 0, { what: "B's socket closed" });
  await inRoom(a, (state) => state.room.players === 1, "A alone again");
  await b.shot("left");

  // B comes back by typing the room's code.
  await clickWidget(b, "Room code");
  await b.keyboard.type(room.toLowerCase());
  await untilState(
    b,
    async () => (await findWidget(b, "Room code"))?.value === room.toLowerCase(),
    {
      what: "the typed code",
    },
  );
  await clickWidget(b, "FIND");
  await roomChosen(b, room);
  await clickWidget(b, "JOIN ROOM");
  bState = await inRoom(b, (state) => state.room.players === 2, "B in by the typed code");
  await inRoom(a, (state) => state.room.players === 2, "A sees B again");

  // A reload opens Battle Setup with the room chosen, its seat held; joining takes it.
  const seatBefore = await seat(b, room);
  const playerBefore = bState.extras.debug.connection.playerId;
  await openAddress(b, { reload: true });
  await roomChosen(b, room);
  assert.equal(query(b).get("room"), room, "the reload keeps the room's address");
  assert.deepEqual(await seat(b, room), seatBefore, "the seat survives the reload");
  const held = await listed(room, (listing) => listing?.reserved === 2, "the seat held");
  observations.reloadHeld = held;
  await b.shot("reloaded");
  await clickWidget(b, "JOIN ROOM");
  bState = await inRoom(b, (state) => state.room.players === 2, "B back after the reload");
  assert.equal(bState.extras.debug.connection.playerId, playerBefore, "the reserved seat retaken");
  observations.rejoined = await listed(
    room,
    (listing) => listing?.players === 2,
    "both players connected",
  );
  assert.equal(observations.rejoined.reserved, 2, "a reload takes no third seat");
  await noRendererErrors([a, b], "in the room");

  // The host ends the battle: both see the results, then PLAY AGAIN starts round two.
  await a.keyboard.press("Escape");
  await clickWidget(a, "END BATTLE");
  for (const page of [a, b]) {
    await untilState(page, (state) => state.room.phase === "Results", { what: "the results" });
  }
  await a.shot("results");
  await clickWidget(a, "PLAY AGAIN");
  for (const page of [a, b]) {
    const next = await inRoom(
      page,
      (state) => state.extras.debug.connection.roundId === 2 && state.room.players === 2,
      "the second battle",
    );
    assert.equal(next.room.menuOpen, false, "the second battle starts with the menu closed");
  }

  // A crash: CONNECTION LOST while the clients retry. The restarted server holds no
  // seats, so the creator's reconnect hosts a fresh lobby there; a player who came by the
  // link and reconnects first finds no room and is told it ended, one who comes after the
  // creator joins the fresh lobby (`MatchHost::join`), so B may end either way.
  const epoch = (await nextFrame(a, ["debug"])).extras.debug.connection.roomEpoch;
  outage = true;
  await server.stop("SIGKILL");
  for (const page of [a, b]) {
    await untilState(page, (state) => /CONNECTION LOST/.test(state.room.link), {
      what: "CONNECTION LOST after the crash",
    });
  }
  await a.shot("server-down");
  await server.start();
  aState = await untilState(
    a,
    (state) => state.room.link === "Live" && state.extras.debug.connection.roomEpoch !== epoch,
    { extras: ["debug"], timeout: 40000, what: "A in the restarted server's room" },
  );
  assert.equal(aState.room.phase, "Lobby", "the restarted server's room is a fresh lobby");
  bState = await untilState(
    b,
    (state) => state.room.link === "Live" || /ROOM CLOSED/.test(state.room.link),
    { timeout: 40000, what: "B in the fresh lobby or told the room ended" },
  );
  observations.bAfterRestart =
    bState.room.link === "Live" ? "joined the fresh lobby" : "told the room ended";
  outage = false;
  await a.shot("restarted");
  await clickWidget(a, "START BATTLE");
  await inRoom(a, () => true, "a battle on the restarted server");
  observations.restarted = await listed(room, (listing) => listing, "the room again");
  await noRendererErrors([a], "after the restart");

  // A deploy's graceful stop ends the room: ROOM CLOSED, and the way back to Battle Setup.
  outage = true;
  await server.stop("SIGTERM");
  for (const page of [a, b]) {
    await untilState(page, (state) => /ROOM CLOSED/.test(state.room.link), {
      what: "ROOM CLOSED",
    });
  }
  await untilState(a, () => findWidget(a, "BATTLE SETUP"), { what: "ROOM CLOSED's way back" });
  assert.equal(await findWidget(a, "TRY AGAIN"), undefined, "a closed room offers no retry");
  await a.shot("room-closed");
  await clickWidget(a, "BATTLE SETUP");
  await untilState(a, (state) => state.engine === "single" && state.multiplayerTab, {
    what: "Battle Setup after the room closed",
  });
  assert.equal(query(a).get("room"), room, "a closed room keeps its address");
  await a.shot("back-from-closed-room");
  await b.context().close();

  // A directory listing a server of another content version: its room cannot be chosen
  // and no new room can be made there.
  const directoryPort = await freePort();
  const outdated = { server: `ws://127.0.0.1:${directoryPort}`, room: "OLDR2345" };
  const directory = http.createServer((_, response) => {
    response.writeHead(200, {
      "content-type": "application/json",
      "access-control-allow-origin": "*",
    });
    response.end(
      JSON.stringify({
        servers: [
          {
            url: outdated.server,
            protocol: 7,
            contentVersion: "older-game",
            rooms: [
              {
                room: outdated.room,
                contentVersion: "older-game",
                mapMode: "harbor",
                difficulty: "normal",
                humansOnly: true,
                roundMinutes: 10,
                players: 2,
                reserved: 2,
                phase: "playing",
                roundId: 3,
                time: 200,
                scores: [4, 6],
              },
            ],
          },
        ],
      }),
    );
  });
  await new Promise((resolve) => directory.listen(directoryPort, "127.0.0.1", resolve));
  try {
    for (const link of ["multiplayer", `room=${outdated.room}`]) {
      await openShell(a, `directory=http://127.0.0.1:${directoryPort}&${link}`);
      await untilState(a, () => findWidget(a, outdated.room), { what: "the outdated room" });
      const row = await findWidget(a, outdated.room);
      assert.equal(row.enabled, false, `${link}: the outdated room cannot be chosen`);
      assert.equal(row.selected, false, `${link}: the outdated room is not chosen`);
      assert.equal(await findWidget(a, "JOIN ROOM"), undefined, `${link}: no JOIN ROOM`);
      const create = await findWidget(a, "CREATE ROOM");
      assert.equal(create?.enabled, false, `${link}: no new room on an outdated server`);
      await a.shot(`outdated-${link.split("=")[0]}`);
    }
  } finally {
    directory.close();
  }
  outage = false;

  assert.deepEqual(errors, [], "page and console errors");
  console.log(JSON.stringify({ room, ...observations, outageNoise: noise.length }));
  console.log(
    "multiplayer-check: create, invite link, join by link and by code, both drive, delay, " +
      "drop and reconnect, leave, reload seat, results and a second battle, crash and " +
      "restart, room closed and an outdated server passed",
  );
} finally {
  writeFileSync(`${out}/result.json`, JSON.stringify({ observations, errors, noise }, null, 2));
  writeFileSync(`${out}/server.log`, server.logs());
  await browser.close();
  await server.stop();
}
