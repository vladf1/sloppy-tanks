import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { StateMirror } from "../src/net/replication.ts";
import { BOT_NAMES } from "../src/game/bot-personalities.ts";
import { checkMultiplayerMenu } from "./multiplayer-ui-assertions.mjs";

const base = process.env.SLOPPY_URL ?? "http://127.0.0.1:5175/sloppy-tanks/";
const label = process.env.SLOPPY_CHECK_LABEL ?? "local";
assert.match(label, /^[a-z0-9-]+$/);
const output = `artifacts/performance/multiplayer/rooms-${label}`;
await mkdir(output, { recursive: true });
const browser = await chromium.launch({
  channel: "chrome",
  headless: false,
  args: [
    "--disable-background-timer-throttling",
    "--disable-renderer-backgrounding",
    "--disable-backgrounding-occluded-windows",
  ],
});
const errors = [],
  clients = [];
const click = async (page, selector) => {
  await page.locator(selector).scrollIntoViewIfNeeded();
  const bounds = await page.locator(selector).boundingBox();
  assert.ok(bounds);
  await page.mouse.click(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
};
const until = async (condition, message) => {
  const deadline = Date.now() + 60000;
  while (!condition() && Date.now() < deadline)
    await new Promise((resolve) => setTimeout(resolve, 50));
  assert.ok(condition(), message);
};
try {
  for (let i = 0; i < 2; i++) {
    const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
    await page.addInitScript(() => Object.defineProperty(document, "hidden", { get: () => false }));
    const client = { page, mirror: new StateMirror(), updates: 0, listRequests: 0 };
    clients.push(client);
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("request", (request) => {
      if (new URL(request.url()).pathname === "/rooms") client.listRequests++;
    });
    page.on("websocket", (socket) => {
      socket.on("framereceived", ({ payload }) => {
        try {
          const message = JSON.parse(String(payload));
          if (message.type === "lobby") client.lobby = message;
          if (message.type === "control") client.control = message;
          if (message.type === "full") {
            client.mirror.applyFull(message, client.lobby);
            client.updates++;
          }
          if (message.type === "snapshot") {
            client.updates++;
            for (const snapshot of message.snapshots)
              assert.ok(client.mirror.applySnapshot(snapshot));
          }
          if (message.type === "error" || message.type === "room-reset") errors.push(message);
        } catch (error) {
          errors.push(error.message);
        }
      });
    });
  }
  const [alice, bob] = clients;
  await alice.page.goto(base);
  await click(alice.page, "#multiplayer-entry");
  await alice.page.locator(".room-browser").waitFor();
  assert.ok(BOT_NAMES.includes(await alice.page.locator("#player-name").inputValue()));
  assert.equal(await alice.page.locator("#player-team").inputValue(), "auto");
  assert.equal(await alice.page.locator("#create-humans-only").isChecked(), true);
  await checkMultiplayerMenu(alice.page);
  await alice.page.locator("#player-name").fill("Room browser Alice");
  await alice.page.locator("#create-map").selectOption("harbor");
  await alice.page.screenshot({ path: `${output}/create.png` });
  await click(alice.page, "#create-room");
  await until(
    () => alice.control?.controlEpoch >= 2 && alice.lobby?.phase === "playing",
    "Create starts battle and prepares input",
  );
  assert.equal(alice.lobby.settings.mapMode, "harbor");
  assert.equal(alice.mirror.state.entities.tanks.length, 1);
  const room = new URL(alice.page.url()).searchParams.get("room");
  const directoryURL = new URL(base);
  directoryURL.searchParams.set("multiplayer", "");
  await bob.page.goto(directoryURL.href);
  const roomSelector = `input[name="room-choice"][value="${room}"]`;
  await bob.page.locator(roomSelector).waitFor();
  await checkMultiplayerMenu(bob.page);
  const row = bob.page.locator(".room-row").filter({ has: bob.page.locator(roomSelector) });
  assert.match(await row.innerText(), /Harbor Havoc · 1\/8 players/);
  assert.match(await row.innerText(), /Humans only/);
  await bob.page.screenshot({ path: `${output}/room-list.png` });
  await bob.page.locator("#player-name").fill("Room browser Bob");
  await click(bob.page, roomSelector);
  await click(bob.page, "#join-room");
  await until(
    () => bob.control?.controlEpoch >= 2 && alice.lobby?.players.length === 2,
    "Join enters the running game",
  );
  assert.deepEqual(
    alice.lobby.players.map((player) => player.team),
    [0, 1],
  );
  assert.equal(bob.mirror.state.entities.tanks.length, 2);
  const polls = clients.map((client) => client.listRequests);
  await until(() => clients.every((client) => client.updates > 10), "Clients receive updates");
  await click(alice.page, "#nerd-stats button");
  await alice.page.waitForFunction(() =>
    [...document.querySelectorAll("#nerd-stats pre")].some((row) =>
      /^Updates received\s+[1-9]/.test(row.textContent),
    ),
  );
  const count = () =>
    alice.page
      .locator("#nerd-stats pre")
      .filter({ hasText: /^Updates received/ })
      .innerText()
      .then((text) => Number(text.match(/\d+$/)[0]));
  const before = await count();
  await alice.page.waitForTimeout(1200);
  assert.ok((await count()) > before, "Received-update counter advances");
  const render = alice.page
    .locator("#nerd-stats details")
    .filter({ has: alice.page.locator("summary", { hasText: /^Render$/ }) });
  assert.match(await render.innerText(), /GPU geometries/);
  assert.match(await render.innerText(), /GPU textures/);
  const stats = await alice.page.locator("#nerd-stats-details").innerText();
  assert.doesNotMatch(stats, /Backend/);
  assert.match(stats, /Input seq sent \/ ack/);
  await alice.page.screenshot({ path: `${output}/stats.png` });
  await alice.page.keyboard.press("n");
  assert.equal(await alice.page.locator("#nerd-stats-details").isVisible(), false);
  await alice.page.waitForTimeout(5100);
  assert.deepEqual(
    clients.map((client) => client.listRequests),
    polls,
    "Directory polling stops during battle",
  );
  for (const client of clients) {
    await click(client.page, "#pause");
    await click(client.page, "#leave-room");
    await client.page.locator(".room-browser").waitFor();
    assert.equal(
      await client.page.locator("#player-name").inputValue(),
      client === alice ? "Room browser Alice" : "Room browser Bob",
    );
  }
  await alice.page.locator(roomSelector).waitFor({ state: "detached", timeout: 15000 });
  await click(bob.page, "#refresh-rooms");
  await bob.page.locator(roomSelector).waitFor({ state: "detached", timeout: 15000 });
  assert.deepEqual(errors, []);
  console.log(
    "Room browser: random/saved names, responsive UI, create/start selected map, listing, Auto teams, late join, stats counters, no in-game polling and empty-room removal passed.",
  );
} finally {
  await writeFile(
    `${output}/result.json`,
    JSON.stringify(
      {
        base,
        errors,
        clients: clients.map((client) => ({
          updates: client.updates,
          listRequests: client.listRequests,
        })),
      },
      null,
      2,
    ),
  );
  await browser.close();
}
