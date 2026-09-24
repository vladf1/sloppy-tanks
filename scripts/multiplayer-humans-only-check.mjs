import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { headless } from "./browser-helpers.mjs";
import { checkMultiplayerMenu } from "./multiplayer-ui-assertions.mjs";

const output = "artifacts/performance/multiplayer/humans-only";
await mkdir(output, { recursive: true });
const url = new URL(process.env.SLOPPY_URL ?? "http://127.0.0.1:5175/sloppy-tanks/");
url.searchParams.set(
  "room",
  [...crypto.getRandomValues(new Uint8Array(8))]
    .map((n) => "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"[n & 31])
    .join(""),
);
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
const errors = [];
const pages = [];
let invite;
async function join(name, team) {
  const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  pages.push(page);
  await page.addInitScript(() => Object.defineProperty(document, "hidden", { get: () => false }));
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(invite ?? url.href);
  await page.locator("#player-name").fill(name);
  invite ??= page.url();
  await page.locator("#player-team").selectOption(String(team));
  await page.locator("#join-room").click();
  await page.waitForFunction(() => document.querySelector("#player-name").disabled);
  return page;
}
async function playing(page, count, round = 1) {
  await page.waitForFunction(
    ({ count, round }) => {
      const game = window.sloppyMultiplayer;
      return (
        game?.control?.driver === "human" &&
        game.control.controlEpoch >= 2 &&
        game.display?.match.phase === "playing" &&
        game.connection.roundId === round &&
        game.display.tanks.length === count
      );
    },
    { count, round },
    { timeout: 60000 },
  );
}
try {
  const alice = await join("Alice", 0);
  const bob = await join("Bob", 1);
  const checkbox = alice.locator("#room-humans-only");
  assert.equal(await checkbox.isChecked(), false);
  assert.equal(await bob.locator("#room-humans-only").isDisabled(), true);
  const bounds = await checkbox.boundingBox();
  await alice.mouse.click(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
  await bob.waitForFunction(() => document.querySelector("#room-humans-only").checked);
  await alice.waitForFunction(() => document.querySelector("#room-difficulty").disabled);
  assert.match(await bob.locator("#network-roster").innerText(), /open seats/);
  assert.equal(await alice.locator("#room-difficulty").isDisabled(), true);
  await checkMultiplayerMenu(alice);
  await alice.screenshot({ path: `${output}/lobby.png` });
  await alice.locator("#start-match").click();
  await Promise.all([playing(alice, 2), playing(bob, 2)]);
  await bob.bringToFront();
  const before = await bob.evaluate(() => window.sloppyMultiplayer.display.viewer.position);
  await bob.keyboard.down("a");
  await bob.waitForTimeout(1000);
  await bob.keyboard.up("a");
  const after = await bob.evaluate(() => window.sloppyMultiplayer.display.viewer.position);
  assert.ok(Math.hypot(after.x - before.x, after.z - before.z) > 0.4);
  await alice.locator("#pause").click();
  await alice.waitForFunction(() => window.sloppyMultiplayer.control.driver === "idle");
  assert.match(await alice.locator("#network-help").innerText(), /idle and vulnerable/);
  assert.equal(await checkbox.isDisabled(), true);
  await alice.locator("#network-resume").click();
  await playing(alice, 2);
  const carol = await join("Carol", 1);
  await Promise.all([playing(alice, 3), playing(bob, 3), playing(carol, 3)]);
  await carol.locator("#pause").click();
  await carol.locator("#leave-room").click();
  await carol.locator("#room-list").waitFor();
  await Promise.all([playing(alice, 2), playing(bob, 2)]);
  await alice.screenshot({ path: `${output}/game.png` });
  await alice.locator("#pause").click();
  await alice.locator("#network-end").click();
  await alice.locator("#network-scoreboard h2").waitFor();
  await checkbox.uncheck();
  await bob.waitForFunction(() => !document.querySelector("#room-humans-only").checked);
  await alice.waitForFunction(() => !document.querySelector("#room-difficulty").disabled);
  assert.equal(await alice.locator("#room-difficulty").isDisabled(), false);
  await alice.locator("#start-match").click();
  await Promise.all([playing(alice, 12, 2), playing(bob, 12, 2)]);
  await alice.locator("#pause").click();
  await alice.locator("#network-end").click();
  assert.deepEqual(errors, []);
  console.log(
    "Humans-only: host checkbox, guest sync, idle pause, movement, late join, departure and restoring bots passed.",
  );
} finally {
  await writeFile(`${output}/result.json`, JSON.stringify({ errors }, null, 2));
  await browser.close();
}
