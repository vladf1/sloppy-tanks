import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { StateMirror } from "./state-mirror.mjs";
import {
  DEFAULT_GAME_URL,
  assertJoinedBehindSetup,
  checkMultiplayerMenu,
  chooseRoomMap,
  click,
  joinFrames,
  launchChrome,
  openMultiplayerTab,
  randomRoomCode,
  recordJoinFrames,
  recordRoomFrames,
  waitForRoomBrowser,
} from "./multiplayer-helpers.mjs";

const baseURL = new URL(process.env.SLOPPY_URL ?? DEFAULT_GAME_URL);
if (process.env.SLOPPY_SERVER) baseURL.searchParams.set("server", process.env.SLOPPY_SERVER);
const base = baseURL.href;
const label = process.env.SLOPPY_CHECK_LABEL ?? "local";
assert.match(label, /^[a-z0-9-]+$/);
const output = `artifacts/performance/multiplayer/rooms-${label}`;
await mkdir(output, { recursive: true });
const browser = await launchChrome();
const errors = [],
  clients = [];
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
    await recordJoinFrames(page);
    const client = recordRoomFrames(page, errors, { mirror: new StateMirror() });
    Object.assign(client, { page, listRequests: 0 });
    clients.push(client);
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("request", (request) => {
      if (new URL(request.url()).pathname === "/rooms") client.listRequests++;
    });
  }
  const [alice, bob] = clients;
  await alice.page.goto(base);
  await openMultiplayerTab(alice.page);
  assert.ok(new URL(alice.page.url()).searchParams.has("multiplayer"), "The tab is kept on reload");
  assert.equal(await alice.page.locator("#tab-single").getAttribute("aria-selected"), "false");
  assert.equal(await alice.page.locator("#start").isVisible(), false, "GO is single-player only");
  // A fresh visitor gets one of the bot roster's names (`src/net/player-name.ts`).
  assert.match(await alice.page.locator("#player-name").inputValue(), /^[A-Z][A-Z ]{1,23}$/);
  assert.equal(await alice.page.locator('input[name="playerTeam"]:checked').inputValue(), "auto");
  assert.equal(await alice.page.locator("#create-humans-only").isChecked(), true);
  assert.equal(await alice.page.locator("#create-round-minutes").inputValue(), "20");
  await checkMultiplayerMenu(alice.page);
  await alice.page.locator("#player-name").fill("Room browser Alice");
  // The tank cards are shared by both tabs and choose the multiplayer tank too.
  await click(alice.page, '[data-kind="heavy"]');
  await chooseRoomMap(alice.page, "harbor");
  await alice.page.locator("#create-round-minutes").fill("3");
  await alice.page.screenshot({ path: `${output}/create.png` });
  // This page began building a single-player arena, so the room opens in a fresh page.
  await click(alice.page, "#create-room");
  await until(
    () => alice.control?.controlEpoch >= 2 && alice.lobby?.phase === "playing",
    "Create starts battle and prepares input",
  );
  assert.equal(alice.lobby.settings.mapMode, "harbor");
  assert.equal(alice.lobby.players[0].kind, "heavy");
  assert.equal(alice.lobby.players[0].name, "Room browser Alice");
  assert.equal(new URL(alice.page.url()).searchParams.has("multiplayer"), false);
  assert.equal(
    await alice.page.evaluate(() => sessionStorage.getItem("sloppy-pending-join")),
    null,
    "The room page consumes the pending join once",
  );
  assert.equal(alice.lobby.settings.roundMinutes, 3);
  // The reloaded page shows the setup the room was created from until its arena is ready.
  const aliceFrames = await joinFrames(alice.page);
  assertJoinedBehindSetup(aliceFrames, "Create after a reload");
  const restored = aliceFrames.find((frame) => frame.joining);
  assert.equal(restored?.play, "multiplayer", "The reloaded setup stays on the multiplayer tab");
  assert.equal(restored.kind, "heavy", "The reloaded setup keeps the chosen tank");
  assert.ok(alice.mirror.state.match.time <= 180 && alice.mirror.state.match.time > 160);
  await alice.page.locator("#network-players").waitFor({ state: "visible" });
  assert.equal(await alice.page.locator(".network-player").count(), 1);
  assert.equal(await alice.page.locator(".network-player b").innerText(), "0");
  assert.equal(alice.mirror.state.entities.tanks.length, 1);
  const room = new URL(alice.page.url()).searchParams.get("room");
  // A link to a room that isn't open still opens Battle Setup, and says so.
  const missingURL = new URL(base);
  missingURL.searchParams.set("room", randomRoomCode());
  await bob.page.goto(missingURL.href);
  await bob.page.waitForFunction(() =>
    document.querySelector("#rooms-message")?.textContent.includes("isn't open"),
  );
  assert.equal(await bob.page.locator("#tab-multiplayer").getAttribute("aria-selected"), "true");
  assert.equal(
    await bob.page.locator('input[name="room-choice"]:checked').getAttribute("id"),
    "new-room",
    "A missing room leaves the new room chosen",
  );
  assert.equal(await bob.page.locator("#join-room").isDisabled(), true);
  assert.equal(await bob.page.locator("#create-room").isVisible(), true);
  const directoryURL = new URL(base);
  directoryURL.searchParams.set("multiplayer", "");
  await bob.page.goto(directoryURL.href);
  assert.equal(await bob.page.locator("#tab-multiplayer").getAttribute("aria-selected"), "true");
  const roomSelector = `input[name="room-choice"][value="${room}"]`;
  await bob.page.locator(roomSelector).waitFor();
  await checkMultiplayerMenu(bob.page);
  const row = bob.page.locator(".room-row").filter({ has: bob.page.locator(roomSelector) });
  assert.match(await row.innerText(), /Harbor Havoc · 1\/8 players/);
  assert.match(await row.innerText(), /Humans only/);
  assert.match(await row.innerText(), /3 min/);
  await bob.page.screenshot({ path: `${output}/room-list.png` });
  await bob.page.locator("#player-name").fill("Bob <b>literal</b>");
  await click(bob.page, '[data-kind="scout"]');
  await click(bob.page, roomSelector);
  // An open room keeps its own map: the map choice shows that map and waits.
  assert.equal(await bob.page.locator(".map-choice").evaluate((part) => part.inert), true);
  assert.equal(await bob.page.locator('.room-map input[name="mapMode"]').inputValue(), "harbor");
  // No single-player arena was built on a ?multiplayer page, so Join stays in this page.
  const bobDocument = await bob.page.evaluate(() => performance.timeOrigin);
  await click(bob.page, "#join-room");
  await until(
    () => bob.control?.controlEpoch >= 2 && alice.lobby?.players.length === 2,
    "Join enters the running game",
  );
  assert.deepEqual(
    alice.lobby.players.map((player) => player.team),
    [0, 1],
  );
  assert.equal(alice.lobby.players[1].kind, "scout");
  assert.equal(await bob.page.evaluate(() => performance.timeOrigin), bobDocument);
  assertJoinedBehindSetup(await joinFrames(bob.page), "In-page join");
  assert.equal(bob.mirror.state.entities.tanks.length, 2);
  await alice.page.waitForFunction(() =>
    document.querySelector("#feed")?.textContent.includes("Bob <b>literal</b> joined Red team"),
  );
  for (const client of clients) {
    assert.equal(await client.page.locator(".network-player").count(), 2);
    assert.match(await client.page.locator("#network-players").innerText(), /Bob <b>literal<\/b>/);
    assert.equal(
      await client.page.locator(".network-player span b").count(),
      0,
      "Names remain literal text",
    );
    assert.deepEqual(await client.page.locator(".network-player b").allTextContents(), ["0", "0"]);
    assert.match(await client.page.locator("#network-summary").textContent(), /3 min/);
    assert.equal(await client.page.locator("#host-settings").isVisible(), false);
  }
  await alice.page.screenshot({ path: `${output}/players-and-join.png` });
  if (new URL(base).hostname === "127.0.0.1") {
    const score = await alice.page.evaluate(() => {
      const game = window.sloppyMultiplayer,
        original = game.hud;
      game.ui.update(
        {
          ...original,
          scoreboard: original.scoreboard.map((tank) => ({
            ...tank,
            kills: tank.id === original.human.id ? 4 : tank.kills,
          })),
        },
        0,
        true,
      );
      const text = document.querySelector(
        `[data-player-id="${game.connection.playerId}"] b`,
      ).textContent;
      game.ui.update(original, 0, true);
      return text;
    });
    assert.equal(score, "4", "The live list renders kill updates from the engine's HUD record");
  }
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
  assert.doesNotMatch(
    await alice.page.locator("#feed").innerText(),
    /joined Red team/,
    "Join notice expires",
  );
  assert.deepEqual(
    clients.map((client) => client.listRequests),
    polls,
    "Directory polling stops during battle",
  );
  for (const client of clients) {
    await click(client.page, "#pause");
    await click(client.page, "#leave-room");
    await client.page.locator("#multiplayer-panel:not([hidden])").waitFor();
    await waitForRoomBrowser(client.page);
    assert.equal(
      await client.page.locator("#player-name").inputValue(),
      client === alice ? "Room browser Alice" : "Bob <b>literal</b>",
    );
  }
  await alice.page.locator(roomSelector).waitFor({ state: "detached", timeout: 15000 });
  await click(bob.page, "#refresh-rooms");
  await bob.page.locator(roomSelector).waitFor({ state: "detached", timeout: 15000 });
  // Arrow keys move between tabs; the single-player tab stops room polling.
  await bob.page.locator("#tab-multiplayer").focus();
  await bob.page.keyboard.press("ArrowLeft");
  await bob.page.locator("#single-panel:not([hidden])").waitFor();
  assert.equal(
    await bob.page.locator("#tab-single").evaluate((tab) => tab === document.activeElement),
    true,
  );
  assert.equal(new URL(bob.page.url()).searchParams.has("multiplayer"), false);
  const pausedPolls = bob.listRequests;
  await bob.page.waitForTimeout(6000);
  assert.equal(bob.listRequests, pausedPolls, "The single-player tab does not poll rooms");
  await bob.page.keyboard.press("ArrowRight");
  await bob.page.locator("#multiplayer-panel:not([hidden])").waitFor();
  await until(() => bob.listRequests > pausedPolls, "The multiplayer tab polls again");
  // Extra levels: offered and listed only with ?debug, yet a room link works anywhere.
  const extraLevels = new URL(base);
  extraLevels.searchParams.set("debug", "");
  extraLevels.searchParams.set("multiplayer", "");
  await alice.page.goto(extraLevels.href);
  await waitForRoomBrowser(alice.page);
  assert.equal(await alice.page.locator("#create-humans-only").isChecked(), true);
  await chooseRoomMap(alice.page, "superstress");
  assert.equal(
    await alice.page.locator("#create-humans-only").isChecked(),
    false,
    "An extra level fills its roster with bots by default",
  );
  assert.equal(
    await alice.page.locator(".new-room-map").textContent(),
    "Scrap Yard",
    "The new room names the shared map",
  );
  await click(alice.page, "#create-room");
  await until(
    () => alice.lobby?.settings.mapMode === "superstress" && alice.lobby.phase === "playing",
    "An extra-level room starts on its level",
  );
  const yard = new URL(alice.page.url()).searchParams.get("room");
  const yardSelector = `input[name="room-choice"][value="${yard}"]`;
  const plainRequests = bob.listRequests;
  await click(bob.page, "#refresh-rooms");
  await until(() => bob.listRequests > plainRequests, "The plain page lists rooms again");
  await bob.page.waitForTimeout(500);
  assert.equal(await bob.page.locator(yardSelector).count(), 0, "Plain pages hide extra levels");
  const link = new URL(base);
  link.searchParams.set("room", yard);
  await bob.page.goto(link.href);
  await bob.page.locator(`${yardSelector}:checked`).waitFor();
  // Only the dropdown names an extra level, so it stands in for the row of standard maps.
  assert.equal(await bob.page.locator(".map-row").isVisible(), false);
  assert.match(await bob.page.locator(".map-picker-current").innerText(), /^Scrap Yard/);
  const yardRow = bob.page.locator(".room-row").filter({ has: bob.page.locator(yardSelector) });
  assert.match(await yardRow.locator("strong").innerText(), /^Scrap Yard\s*EXTRA · 1\/8 players$/);
  await click(alice.page, "#pause");
  await click(alice.page, "#leave-room");
  await alice.page.locator("#multiplayer-panel:not([hidden])").waitFor();
  assert.deepEqual(errors, []);
  console.log(
    "Room browser: tabs, shared tank cards, random/saved names, responsive UI, create/start selected map after a reload, in-page join, listing, Auto teams, live player kills, join notifications, match length, late join, stats counters, no in-game or single-player polling, empty-room removal and extra-level rooms passed.",
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
