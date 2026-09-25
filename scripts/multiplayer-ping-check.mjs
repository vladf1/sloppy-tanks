import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { headless } from "./browser-helpers.mjs";

const output = "artifacts/performance/multiplayer/ping-relay";
await mkdir(output, { recursive: true });
const url = new URL(process.env.SLOPPY_URL ?? "http://127.0.0.1:5175/sloppy-tanks/");
const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const room = [...crypto.getRandomValues(new Uint8Array(8))].map((n) => alphabet[n & 31]).join("");
url.searchParams.set("room", room);
if (process.env.SLOPPY_SERVER) url.searchParams.set("server", process.env.SLOPPY_SERVER);
const browser = await chromium.launch({
  channel: "chrome",
  headless,
  args: [
    "--disable-background-timer-throttling",
    "--disable-renderer-backgrounding",
    "--disable-backgrounding-occluded-windows",
  ],
});
const samples = [],
  errors = [],
  pages = [];
try {
  for (const name of ["Ping Alice", "Ping Bob"]) {
    const context = await browser.newContext({ viewport: { width: 1100, height: 780 } });
    const page = await context.newPage();
    pages.push(page);
    await page.addInitScript(() => Object.defineProperty(document, "hidden", { get: () => false }));
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("websocket", (socket) => {
      if (!socket.url().includes("/room/")) return;
      const sent = new Map();
      socket.on("framesent", ({ payload }) => {
        const m = JSON.parse(String(payload));
        if (m.type === "ping") sent.set(m.t, performance.now());
      });
      socket.on("framereceived", ({ payload }) => {
        const m = JSON.parse(String(payload));
        if (m.type !== "pong") return;
        const started = sent.get(m.t);
        sent.delete(m.t);
        samples.push({
          name,
          ...m,
          observedRttMs: started === undefined ? null : performance.now() - started,
        });
      });
    });
    await page.goto(url.href);
    await page.locator("#player-name").fill(name);
    await page.locator("#join-room").click();
    await page.waitForFunction(() => window.sloppyMultiplayer?.connection.connected);
  }
  const [alice, bob] = pages;
  await alice.locator("#start-match").click();
  await Promise.all(
    pages.map((page) =>
      page.waitForFunction(
        () => {
          const game = window.sloppyMultiplayer;
          return (
            game?.display?.match.phase === "playing" &&
            game.control?.driver === "human" &&
            !document.querySelector("#startup-overlay")
          );
        },
        null,
        { timeout: 60000 },
      ),
    ),
  );
  const initial = await alice.evaluate(() => ({
    ...window.sloppyMultiplayer.display.viewer.position,
  }));
  await alice.bringToFront();
  await alice.keyboard.down("KeyW");
  await alice.waitForTimeout(1200);
  await alice.keyboard.up("KeyW");
  const moved = await alice.evaluate(() => ({
    ...window.sloppyMultiplayer.display.viewer.position,
  }));
  assert.ok(
    Math.hypot(moved.x - initial.x, moved.z - initial.z) > 0.5,
    "Player moves through relay",
  );
  await alice.waitForTimeout(10000);
  const playerId = await bob.evaluate(() => window.sloppyMultiplayer.connection.playerId);
  await bob.evaluate(() => window.sloppyMultiplayer.connection.socket.close());
  await bob.waitForFunction(() => !window.sloppyMultiplayer.connection.connected);
  await bob.waitForFunction(() => window.sloppyMultiplayer.connection.connected);
  assert.equal(await bob.evaluate(() => window.sloppyMultiplayer.connection.playerId), playerId);
  await bob.waitForTimeout(2000);
  for (const sample of samples) {
    assert.ok(
      Number.isFinite(sample.workerToRoomMs) && sample.workerToRoomMs >= 0,
      JSON.stringify(sample),
    );
  }
  assert.ok(samples.length >= 20, "Collected recurring pongs with stats panel closed");
  assert.deepEqual(errors, []);
  await alice.screenshot({ path: `${output}/playing.png` });
  const sorted = samples.map((sample) => sample.workerToRoomMs).sort((a, b) => a - b);
  const report = {
    url: url.href,
    samples,
    errors,
    movementMetres: Math.hypot(moved.x - initial.x, moved.z - initial.z),
    reconnectRetainedSeat: true,
    workerToRoomMs: {
      min: sorted[0],
      median: sorted[Math.floor(sorted.length / 2)],
      max: sorted.at(-1),
    },
  };
  await writeFile(`${output}/result.json`, JSON.stringify(report, null, 2));
  console.log(JSON.stringify({ ...report, samples: samples.length }, null, 2));
} finally {
  await browser.close();
}
