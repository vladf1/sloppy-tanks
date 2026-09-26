import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import {
  DEFAULT_GAME_URL,
  checkMultiplayerMenu,
  click,
  launchChrome,
  randomRoomCode,
  recordRoomFrames,
} from "./multiplayer-helpers.mjs";

const output = "artifacts/performance/multiplayer/browser";
await mkdir(output, { recursive: true });
const url = new URL(process.env.SLOPPY_URL ?? DEFAULT_GAME_URL);
url.searchParams.set("room", randomRoomCode());
if (process.env.SLOPPY_SERVER) url.searchParams.set("server", process.env.SLOPPY_SERVER);
for (const [env, key] of [
  ["SLOPPY_LATENCY", "latency"],
  ["SLOPPY_JITTER", "jitter"],
  ["SLOPPY_STALL", "stall"],
]) {
  if (process.env[env]) url.searchParams.set(key, process.env[env]);
}
/** The client refreshes unchanged input about once per second. */
const IDLE_WINDOW_MS = 6500;
const browser = await launchChrome();
const errors = [],
  pages = [],
  frames = new Map(),
  observations = {};
const state = (page) =>
  page.evaluate(() => {
    const game = window.sloppyMultiplayer;
    return {
      tick: game.mirror.tick,
      player: game.connection.playerId,
      control: game.control,
      position: game.display?.viewer.position,
      phase: game.display?.match.phase,
      round: game.connection.roundId,
      tanks: game.display?.tanks.map((tank) => ({
        id: tank.id,
        life: tank.life,
        alive: tank.alive,
        position: tank.position,
      })),
    };
  });
async function join(name, team) {
  const context = await browser.newContext({ viewport: { width: 1100, height: 780 } });
  const page = await context.newPage();
  pages.push(page);
  // Several visible players on one test machine must keep receiving animation frames.
  await page.addInitScript(() => {
    Object.defineProperty(document, "hidden", {
      configurable: true,
      get: () => window.fixtureHidden ?? false,
    });
  });
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  frames.set(page, recordRoomFrames(page, errors));
  await page.goto(url.href);
  await page.locator("#player-name").fill(name);
  await page.locator("#player-team").selectOption(String(team));
  await page.locator("#join-room").click();
  await page.waitForFunction(() => window.sloppyMultiplayer?.connection.connected);
  return page;
}
/** Waits until the page drives its own tank in a round with `tanks` tanks. */
function playing(page, tanks, round = 1) {
  return page.waitForFunction(
    ({ tanks, round }) => {
      const game = window.sloppyMultiplayer;
      return (
        game?.view &&
        game.control?.driver === "human" &&
        game.control.controlEpoch >= 2 &&
        game.display?.match.phase === "playing" &&
        game.connection.roundId === round &&
        game.display.tanks.length === tanks
      );
    },
    { tanks, round },
    { timeout: 60000 },
  );
}
const driver = (page, value) =>
  page.waitForFunction((value) => window.sloppyMultiplayer.control.driver === value, value);
try {
  const alice = await join("Alice <b>literal</b>", 0);
  const bob = await join("Bob", 1);
  const aliceFrames = frames.get(alice);
  assert.equal(await alice.locator("#network-roster b").count(), 2, "Names are literal text");

  // Humans-only is the host's setting; guests see it synced and read-only.
  const humansOnly = "#room-humans-only";
  assert.equal(await alice.locator(humansOnly).isChecked(), false);
  assert.equal(await bob.locator(humansOnly).isDisabled(), true);
  await click(alice, humansOnly);
  await bob.waitForFunction(() => document.querySelector("#room-humans-only").checked);
  await alice.waitForFunction(() => document.querySelector("#room-difficulty").disabled);
  assert.match(await bob.locator("#network-roster").innerText(), /open seats/);
  await checkMultiplayerMenu(alice);
  await alice.screenshot({ path: `${output}/lobby.png` });
  await alice.locator("#start-match").click();
  await Promise.all(pages.map((page) => playing(page, 2)));

  // Unchanged input is refreshed slowly and never mistaken for an absent player.
  await alice.waitForTimeout(2000);
  const idleStart = aliceFrames.inputs.length,
    idlePings = aliceFrames.pings,
    idleEpoch = aliceFrames.control.controlEpoch;
  await alice.waitForTimeout(IDLE_WINDOW_MS);
  observations.idle = {
    inputs: aliceFrames.inputs.length - idleStart,
    pings: aliceFrames.pings - idlePings,
    milliseconds: IDLE_WINDOW_MS,
    ack: aliceFrames.ack,
    lastSeq: aliceFrames.inputs.at(-1)?.seq,
  };
  assert.ok(
    observations.idle.inputs >= 5 && observations.idle.inputs <= 8,
    `Unchanged idle input is sent about once per second (${observations.idle.inputs})`,
  );
  assert.equal(aliceFrames.control.driver, "human", "Idle player keeps control past the watchdog");
  assert.equal(aliceFrames.control.controlEpoch, idleEpoch, "No takeover/resume cycle when idle");
  assert.ok(aliceFrames.ack >= aliceFrames.inputs.at(-2).seq, "Idle input is acknowledged");

  observations.before = await Promise.all(pages.map(state));
  await alice.evaluate(() => {
    const game = window.sloppyMultiplayer,
      start = { ...game.display.viewer.position };
    const audit = (window.responseAudit = {});
    window.addEventListener(
      "keydown",
      () => {
        audit.moveStart = performance.now();
      },
      { once: true },
    );
    window.addEventListener(
      "pointerdown",
      () => {
        audit.fireStart = performance.now();
      },
      { once: true },
    );
    const event = game.view.event.bind(game.view);
    game.view.event = (value, hit) => {
      if (
        value.type === "shot" &&
        value.id === game.control.tankId &&
        audit.fireStart &&
        !audit.shotMs
      )
        audit.shotMs = performance.now() - audit.fireStart;
      event(value, hit);
    };
    const observe = () => {
      if (
        audit.moveStart &&
        !audit.moveMs &&
        Math.hypot(
          game.display.viewer.position.x - start.x,
          game.display.viewer.position.z - start.z,
        ) > 0.02
      )
        audit.moveMs = performance.now() - audit.moveStart;
      if (!audit.moveMs) requestAnimationFrame(observe);
    };
    requestAnimationFrame(observe);
  });
  const moveStart = aliceFrames.inputs.length;
  await alice.keyboard.down("d");
  await bob.keyboard.down("a");
  await alice.waitForTimeout(1200);
  await alice.keyboard.up("d");
  await bob.keyboard.up("a");
  await alice.waitForTimeout(150);
  observations.after = await Promise.all(pages.map(state));
  observations.movingInputs = aliceFrames.inputs.length - moveStart;
  assert.ok(observations.movingInputs >= 8, "Held movement keeps the active input cadence");
  assert.equal(aliceFrames.inputs.at(-1).moveX, 0, "Release sends neutral input promptly");
  assert.notEqual(observations.before[0].control.tankId, observations.before[1].control.tankId);
  for (let i = 0; i < 2; i++)
    assert.ok(
      Math.hypot(
        observations.after[i].position.x - observations.before[i].position.x,
        observations.after[i].position.z - observations.before[i].position.z,
      ) > 0.5,
      "Each player's movement reaches the server",
    );

  const aimStart = aliceFrames.inputs.length;
  await alice.mouse.move(600, 260);
  await alice.waitForTimeout(150);
  assert.ok(aliceFrames.inputs.length > aimStart, "Aim change wakes input sending");
  await alice.mouse.down();
  await alice.waitForTimeout(1500);
  await alice.mouse.up();
  await alice.waitForTimeout(150);
  assert.ok(
    aliceFrames.inputs.slice(aimStart).filter((input) => input.fire).length >= 5,
    "Held fire is renewed before its lease expires",
  );
  assert.ok(!aliceFrames.inputs.at(-1).fire, "Fire release reaches the server");
  observations.response = await alice.evaluate(() => window.responseAudit);
  assert.ok(
    observations.response.moveMs > 0 && observations.response.moveMs < 1000,
    "Bounded input-to-visible movement",
  );
  assert.ok(
    observations.response.shotMs > 0 && observations.response.shotMs < 1000,
    "Bounded button-to-visible shot effect",
  );
  const mineStart = aliceFrames.inputs.length;
  await alice.mouse.click(600, 260, { button: "right" });
  await alice.waitForFunction(() => window.sloppyMultiplayer.display.viewer.mineCooldown > 0);
  assert.equal(
    aliceFrames.inputs
      .slice(mineStart)
      .flatMap((input) => input.actions ?? [])
      .filter((action) => action.type === "mine").length,
    1,
    "A mine click is sent once",
  );
  await Promise.all(pages.map((page, i) => page.screenshot({ path: `${output}/player-${i}.png` })));

  // Without fill bots, a menu or hidden tab leaves the tank idle rather than bot-driven.
  const epochBeforeMenu = aliceFrames.control.controlEpoch;
  await click(alice, "#pause");
  await driver(alice, "idle");
  assert.match(await alice.locator("#network-help").innerText(), /idle and vulnerable/);
  assert.equal(await alice.locator(humansOnly).isDisabled(), true, "Settings lock during play");
  const pausedTick = (await state(alice)).tick;
  await alice.waitForTimeout(350);
  assert.ok((await state(bob)).tick > pausedTick, "Other player's game continues during menu");
  await click(alice, "#network-resume");
  await driver(alice, "human");
  assert.ok(aliceFrames.control.controlEpoch > epochBeforeMenu, "Resume starts a new epoch");
  await bob.evaluate(() => {
    window.fixtureHidden = true;
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await driver(bob, "idle");
  await bob.evaluate(() => {
    window.fixtureHidden = false;
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await driver(bob, "human");
  const old = await state(bob);
  await bob.reload();
  await bob.locator("#join-room").click();
  await playing(bob, 2);
  observations.reconnected = await state(bob);
  assert.equal(observations.reconnected.player, old.player);
  assert.equal(observations.reconnected.control.tankId, old.control.tankId);
  assert.ok(observations.reconnected.control.controlEpoch > old.control.controlEpoch);

  // A late joiner gets a seat in the running round; leaving frees it at once.
  const carol = await join("Carol", 1);
  await Promise.all(pages.map((page) => playing(page, 3)));
  await carol.locator("#pause").click();
  await carol.locator("#leave-room").click();
  await carol.locator("#room-list").waitFor();
  await Promise.all([alice, bob].map((page) => playing(page, 2)));
  await carol.context().close();
  pages.pop();

  await alice.locator("#pause").click();
  await alice.locator("#network-end").click();
  await Promise.all(pages.map((page) => page.locator("#network-scoreboard h2").waitFor()));
  await alice.screenshot({ path: `${output}/results.png` });
  // Clearing humans-only restores the difficulty choice and fill bots next round.
  await click(alice, humansOnly);
  await bob.waitForFunction(() => !document.querySelector("#room-humans-only").checked);
  await alice.waitForFunction(() => !document.querySelector("#room-difficulty").disabled);
  await alice.locator("#room-map").selectOption("harbor");
  await alice.locator("#start-match").click();
  await Promise.all(pages.map((page) => playing(page, 12, 2)));
  observations.nextRound = await Promise.all(pages.map(state));
  assert.ok(observations.nextRound.every((view) => view.tanks.length === 12));
  const beforeDrop = await state(bob);
  await bob.evaluate(() => window.sloppyMultiplayer.connection.socket.close());
  await bob.waitForFunction(
    (epoch) =>
      window.sloppyMultiplayer.connection.connected &&
      window.sloppyMultiplayer.control?.controlEpoch > epoch &&
      window.sloppyMultiplayer.control.driver === "human",
    beforeDrop.control.controlEpoch,
  );
  assert.equal((await state(bob)).player, beforeDrop.player, "Automatic reconnect retains seat");
  await bob.locator("#pause").click();
  await bob.locator("#touch-mode").selectOption("on");
  await bob.locator("#network-resume").click();
  await bob.locator(".touch-controls").waitFor({ state: "visible" });
  await driver(bob, "human");
  const session = await bob.context().newCDPSession(bob),
    fingers = new Map();
  const center = async (selector) => {
    const b = await bob.locator(selector).boundingBox();
    assert.ok(b);
    return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
  };
  const touch = async (type, id, x, y) => {
    const released = fingers.get(id);
    if (type === "touchEnd") fingers.delete(id);
    else fingers.set(id, { id, x, y, radiusX: 5, radiusY: 5, force: 1 });
    await session.send("Input.dispatchTouchEvent", {
      type,
      touchPoints: type === "touchEnd" ? [released] : [...fingers.values()],
    });
    await bob.evaluate(
      () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
    );
  };
  const drive = await center(".touch-drive"),
    aim = await center(".touch-aim"),
    mine = await center(".touch-mine");
  await touch("touchStart", 1, drive.x, drive.y);
  await touch("touchMove", 1, drive.x - 55, drive.y);
  await touch("touchStart", 2, aim.x, aim.y);
  await touch("touchMove", 2, aim.x, aim.y - 55);
  await touch("touchStart", 3, mine.x, mine.y);
  await touch("touchEnd", 3);
  await bob.waitForFunction(() => window.sloppyMultiplayer.display.viewer.mineCooldown > 0);
  assert.equal(await bob.evaluate(() => window.sloppyMultiplayer.controls.touch.fire), true);
  await bob.screenshot({ path: `${output}/touch.png` });
  await touch("touchEnd", 1);
  await touch("touchEnd", 2);
  assert.equal(await bob.evaluate(() => window.sloppyMultiplayer.controls.touch.fire), false);
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ idle: observations.idle, response: observations.response }));
  console.log(
    "Two-browser multiplayer: join, humans-only sync, idle input, movement, fire, mine, idle menu and hidden tab, reload, late join and leave, results, restored bots on another map, automatic reconnect and real multi-touch passed.",
  );
} finally {
  for (const [i, page] of pages.entries()) {
    await page.screenshot({ path: `${output}/final-${i}.png` }).catch(() => {});
    observations[`final${i}`] = await state(page).catch(() => null);
    observations[`text${i}`] = await page
      .locator("body")
      .innerText()
      .catch(() => null);
  }
  await writeFile(
    `${output}/result.json`,
    JSON.stringify({ url: url.href, errors, observations }, null, 2),
  );
  await browser.close();
}
