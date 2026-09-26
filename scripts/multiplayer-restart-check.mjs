import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { setTimeout as wait } from "node:timers/promises";
import {
  DEFAULT_GAME_URL,
  chooseRoomMap,
  click,
  launchChrome,
  waitForRoomBrowser,
} from "./multiplayer-helpers.mjs";

const output = "artifacts/performance/multiplayer/restart";
await mkdir(output, { recursive: true });
const url = new URL(process.env.SLOPPY_URL ?? DEFAULT_GAME_URL);
const PORT = 8790;
url.searchParams.set("multiplayer", "");
url.searchParams.set("server", `ws://127.0.0.1:${PORT}`);
let server,
  logs = "";
const errors = [];
async function startServer() {
  server = spawn(process.execPath, ["--enable-source-maps", "server/dist/server.mjs"], {
    // Admit whichever Vite origin the check was given, not only the default local ports.
    env: { ...process.env, PORT: String(PORT), ALLOWED_ORIGINS: url.origin },
    stdio: ["ignore", "pipe", "pipe"],
  });
  server.stdout.on("data", (data) => (logs += data));
  server.stderr.on("data", (data) => (logs += data));
  const deadline = Date.now() + 20000;
  while (true) {
    try {
      if ((await fetch(`http://127.0.0.1:${PORT}/health`)).ok) return;
    } catch {
      // The child has not bound its HTTP port yet.
    }
    assert.ok(Date.now() < deadline, "local server starts");
    await wait(100);
  }
}
/**
 * SIGKILL models a crash: sockets drop without the graceful room-reset notice, so the
 * client must reconnect on its own. A deploy (SIGTERM) instead tells players the room ended.
 */
async function stopServer(signal = "SIGKILL") {
  if (!server || server.exitCode !== null || server.signalCode !== null) return;
  const child = server;
  await new Promise((resolve) => {
    child.once("exit", resolve);
    child.kill(signal);
  });
}
const browser = await launchChrome();
try {
  await startServer();
  const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url.href);
  await waitForRoomBrowser(page);
  await page.locator("#player-name").fill("Restart tester");
  await chooseRoomMap(page, "harbor");
  await click(page, "#create-room");
  await page.waitForFunction(
    () =>
      window.sloppyMultiplayer?.display?.mapTheme === "harbor" &&
      document.querySelector("#network-status").textContent === "",
    null,
    { timeout: 60000 },
  );
  const before = await page.evaluate(() => ({
    epoch: window.sloppyMultiplayer.connection.roomEpoch,
    round: window.sloppyMultiplayer.connection.roundId,
  }));
  await stopServer();
  await startServer();
  await page.waitForFunction(
    (epoch) =>
      window.sloppyMultiplayer.connection.roomEpoch !== epoch &&
      window.sloppyMultiplayer.connection.connected,
    before.epoch,
    { timeout: 30000 },
  );
  // The restarted server has no seats, so the reconnect becomes host of a fresh lobby
  // with default settings rather than resuming the harbor round.
  await page.locator("#start-match").waitFor();
  assert.equal(await page.locator("#room-map").inputValue(), "village");
  await click(page, "#start-match");
  await page.waitForFunction(
    () =>
      window.sloppyMultiplayer?.display?.mapTheme === "village" &&
      document.querySelector("#network-status").textContent === "",
    null,
    { timeout: 60000 },
  );
  const after = await page.evaluate(() => ({
    epoch: window.sloppyMultiplayer.connection.roomEpoch,
    round: window.sloppyMultiplayer.connection.roundId,
    feed: document.querySelector("#feed").textContent,
  }));
  assert.equal(before.round, 1);
  assert.equal(after.round, 1);
  assert.notEqual(before.epoch, after.epoch);
  assert.equal(after.feed, "");
  await page.screenshot({ path: `${output}/fresh-room.png` });
  assert.deepEqual(errors, []);
  console.log(
    "Server crash and restart: connection recovers to a fresh lobby; reused round number prepares the new map and clears feedback.",
  );
} finally {
  await browser.close();
  await stopServer("SIGTERM");
  await writeFile(`${output}/server.log`, logs);
}
