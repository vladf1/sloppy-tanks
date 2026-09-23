import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { setTimeout as wait } from "node:timers/promises";
import { chromium } from "playwright";

const output = "artifacts/performance/multiplayer/restart";
await mkdir(output, { recursive: true });
const url = new URL(process.env.SLOPPY_URL ?? "http://127.0.0.1:5175/sloppy-tanks/");
url.searchParams.set("multiplayer", "");
url.searchParams.set("server", "ws://127.0.0.1:8790");
let server,
  logs = "";
const errors = [];
async function startServer() {
  server = spawn(
    process.execPath,
    [
      "node_modules/wrangler/bin/wrangler.js",
      "dev",
      "--config",
      "server/wrangler.jsonc",
      "--port",
      "8790",
      "--inspector-port",
      "9231",
      "--persist-to",
      `${output}/state`,
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  server.stdout.on("data", (data) => (logs += data));
  server.stderr.on("data", (data) => (logs += data));
  const deadline = Date.now() + 20000;
  while (true) {
    try {
      if ((await fetch("http://127.0.0.1:8790/health")).ok) return;
    } catch {
      // The child has not bound its HTTP port yet.
    }
    assert.ok(Date.now() < deadline, "local Worker starts");
    await wait(100);
  }
}
async function stopServer() {
  if (!server || server.exitCode !== null) return;
  const child = server;
  await new Promise((resolve) => {
    child.once("exit", resolve);
    child.kill("SIGTERM");
  });
}
const browser = await chromium.launch({ channel: "chrome", headless: false });
try {
  await startServer();
  const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url.href);
  await page.locator("#player-name").fill("Restart tester");
  await page.locator("#join-room").click();
  await page.locator("#room-map").selectOption("harbor");
  await page.locator("#start-match").click();
  await page.waitForFunction(
    () =>
      window.sloppyMultiplayer?.display?.mapTheme === "harbor" &&
      /^\d+ ms/.test(document.querySelector("#network-status").textContent),
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
  await page.locator("#start-match").click();
  await page.waitForFunction(
    () =>
      window.sloppyMultiplayer?.display?.mapTheme === "village" &&
      /^\d+ ms/.test(document.querySelector("#network-status").textContent),
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
    "Actual Worker restart: connection recovers to a fresh lobby; reused round number prepares the new map and clears feedback.",
  );
} finally {
  await browser.close();
  await stopServer();
  await writeFile(`${output}/worker.log`, logs);
}
