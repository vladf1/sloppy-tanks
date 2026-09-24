import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { headless } from "./browser-helpers.mjs";

const output = "artifacts/performance/multiplayer/browser";
await mkdir(output, { recursive: true });
const url = new URL(process.env.SLOPPY_URL ?? "http://127.0.0.1:5175/sloppy-tanks/");
const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const room = [...crypto.getRandomValues(new Uint8Array(8))].map((n) => alphabet[n & 31]).join("");
url.searchParams.set("room", room);
if (process.env.SLOPPY_SERVER) url.searchParams.set("server", process.env.SLOPPY_SERVER);
if (process.env.SLOPPY_LATENCY) url.searchParams.set("latency", process.env.SLOPPY_LATENCY);
if (process.env.SLOPPY_JITTER) url.searchParams.set("jitter", process.env.SLOPPY_JITTER);
const browser = await chromium.launch({
  channel: "chrome",
  headless,
  args: [
    "--disable-background-timer-throttling",
    "--disable-renderer-backgrounding",
    "--disable-backgrounding-occluded-windows",
  ],
});
const errors = [],
  pages = [],
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
try {
  for (const [i, name] of ["Alice <b>literal</b>", "Bob"].entries()) {
    const context = await browser.newContext({ viewport: { width: 1100, height: 780 } });
    const page = await context.newPage();
    pages.push(page);
    // Two visible players on one test machine must keep receiving animation frames.
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
    await page.goto(url.href);
    await page.locator("#player-name").fill(name);
    await page.locator("#player-team").selectOption(String(i));
    await page.locator("#join-room").click();
    await page.waitForFunction(() => window.sloppyMultiplayer?.connection.connected);
  }
  const [alice, bob] = pages;
  assert.equal(await alice.locator("#network-roster b").count(), 2, "Names are literal text");
  await alice.screenshot({ path: `${output}/lobby.png` });
  await alice.locator("#start-match").click();
  await Promise.all(
    pages.map((page) =>
      page.waitForFunction(
        () => {
          const game = window.sloppyMultiplayer;
          return (
            game?.view &&
            game.display?.match.phase === "playing" &&
            game.control?.driver === "human" &&
            game.control.controlEpoch >= 2
          );
        },
        null,
        { timeout: 60000 },
      ),
    ),
  );
  await alice.waitForTimeout(500);
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
  await alice.keyboard.down("d");
  await bob.keyboard.down("a");
  await alice.waitForTimeout(1200);
  await alice.keyboard.up("d");
  await bob.keyboard.up("a");
  observations.after = await Promise.all(pages.map(state));
  assert.notEqual(observations.before[0].control.tankId, observations.before[1].control.tankId);
  for (let i = 0; i < 2; i++)
    assert.ok(
      Math.hypot(
        observations.after[i].position.x - observations.before[i].position.x,
        observations.after[i].position.z - observations.before[i].position.z,
      ) > 0.5,
      "Each player's movement reaches the server",
    );
  await alice.mouse.move(600, 260);
  await alice.mouse.down();
  await alice.waitForTimeout(1500);
  await alice.mouse.up();
  observations.response = await alice.evaluate(() => window.responseAudit);
  assert.ok(
    observations.response.moveMs > 0 && observations.response.moveMs < 1000,
    "Bounded input-to-visible movement",
  );
  assert.ok(
    observations.response.shotMs > 0 && observations.response.shotMs < 1000,
    "Bounded button-to-visible shot effect",
  );
  await Promise.all(pages.map((page, i) => page.screenshot({ path: `${output}/player-${i}.png` })));
  await alice.locator("#pause").click();
  await alice.waitForFunction(() => window.sloppyMultiplayer.control.driver === "bot");
  const pausedTick = (await state(alice)).tick;
  await alice.waitForTimeout(350);
  assert.ok((await state(bob)).tick > pausedTick, "Other player's game continues during menu");
  await alice.locator("#network-resume").click();
  await alice.waitForFunction(() => window.sloppyMultiplayer.control.driver === "human");
  await bob.evaluate(() => {
    window.fixtureHidden = true;
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await bob.waitForFunction(() => window.sloppyMultiplayer.control.driver === "bot");
  await bob.evaluate(() => {
    window.fixtureHidden = false;
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await bob.waitForFunction(() => window.sloppyMultiplayer.control.driver === "human");
  const old = await state(bob);
  await bob.reload();
  await bob.locator("#join-room").click();
  await bob.waitForFunction(
    () =>
      window.sloppyMultiplayer?.display?.match.phase === "playing" &&
      window.sloppyMultiplayer.control?.driver === "human",
    null,
    { timeout: 60000 },
  );
  observations.reconnected = await state(bob);
  assert.equal(observations.reconnected.player, old.player);
  assert.equal(observations.reconnected.control.tankId, old.control.tankId);
  assert.ok(observations.reconnected.control.controlEpoch > old.control.controlEpoch);
  await alice.locator("#pause").click();
  await alice.locator("#network-end").click();
  await Promise.all(pages.map((page) => page.locator("#network-scoreboard h2").waitFor()));
  await alice.screenshot({ path: `${output}/results.png` });
  await alice.locator("#room-map").selectOption("harbor");
  await alice.locator("#start-match").click();
  await Promise.all(
    pages.map((page) =>
      page.waitForFunction(
        () =>
          window.sloppyMultiplayer?.connection.roundId === 2 &&
          window.sloppyMultiplayer.display?.match.phase === "playing" &&
          window.sloppyMultiplayer.control?.driver === "human",
        null,
        { timeout: 60000 },
      ),
    ),
  );
  observations.nextRound = await Promise.all(pages.map(state));
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
  await bob.waitForFunction(() => window.sloppyMultiplayer.control.driver === "human");
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
  console.log(
    "Two-browser multiplayer: join, movement, fire, pause, hide, reload/automatic reconnect, results, next map and real multi-touch passed.",
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
