// Phones' simplified multiplayer (src/game/phone-mode.ts) against Vite and the local game
// server: the Multiplayer tab shows no room list, only one action. Carol's phone, seeing
// no open room, creates one on her map with bots. Bob's phone starts on single player,
// finds that room on its Multiplayer tab with the room's map shown, joins through a
// reload, and plays it with the phone camera, controls and short menu.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { chooseMap } from "./browser-helpers.mjs";
import { DEFAULT_GAME_URL, launchChrome, recordRoomFrames } from "./multiplayer-helpers.mjs";

const base = new URL(process.env.SLOPPY_URL ?? DEFAULT_GAME_URL);
if (process.env.SLOPPY_SERVER) base.searchParams.set("server", process.env.SLOPPY_SERVER);
const output = "artifacts/performance/phone-multiplayer";
await mkdir(output, { recursive: true });
const PHONE = { width: 874, height: 402 };
const browser = await launchChrome();
const errors = [];
const until = async (condition, message) => {
  const deadline = Date.now() + 60000;
  while (!condition() && Date.now() < deadline)
    await new Promise((resolve) => setTimeout(resolve, 50));
  assert.ok(condition(), message);
};
/** Whether each selector shows. The hidden tab panel keeps its box, so visibility counts. */
const expectVisible = async (page, shown, hidden) => {
  const state = await page.evaluate(
    (selectors) =>
      Object.fromEntries(
        selectors.map((selector) => {
          const element = document.querySelector(selector);
          const box = element?.getBoundingClientRect();
          return [
            selector,
            !!box &&
              box.width > 0 &&
              box.height > 0 &&
              element.checkVisibility({ visibilityProperty: true }),
          ];
        }),
      ),
    [...shown, ...hidden],
  );
  for (const selector of shown) assert.equal(state[selector], true, `${selector} is shown`);
  for (const selector of hidden) assert.equal(state[selector], false, `${selector} is hidden`);
};
/** A phone whose room list keeps only `keep(rooms)` of the server's real list, so other
 * rooms on a shared local server cannot change which room it picks. */
async function openPhone(keep) {
  const context = await browser.newContext({
    viewport: PHONE,
    screen: PHONE,
    isMobile: true,
    hasTouch: true,
  });
  await context.route(
    (url) => url.pathname === "/rooms",
    async (route) => {
      const response = await route.fetch();
      const { rooms } = await response.json();
      await route.fulfill({ response, json: { rooms: keep(rooms) } });
    },
  );
  const page = await context.newPage();
  page.on("pageerror", (error) => errors.push(error.message));
  return { page, room: recordRoomFrames(page, errors) };
}

try {
  const multiplayer = new URL(base);
  multiplayer.searchParams.set("multiplayer", "");
  const carol = await openPhone(() => []);
  await carol.page.goto(multiplayer.href);
  await carol.page.locator('.room-status[data-state="ready"]').waitFor();
  await expectVisible(
    carol.page,
    [".play-tabs", ".map-choice", "#create-room"],
    [".room-browse", ".driver-name", "#join-room", "#start"],
  );
  assert.equal(
    await carol.page.locator("#create-humans-only").isChecked(),
    false,
    "A phone's new room fills its teams with bots",
  );
  await chooseMap(carol.page, "harbor");
  await carol.page.locator("#create-room").tap();
  await until(() => carol.room.lobby?.phase === "playing", "Carol's phone creates a room");
  assert.equal(carol.room.lobby.settings.mapMode, "harbor");
  assert.equal(carol.room.lobby.settings.humansOnly, false);
  assert.equal(carol.room.lobby.settings.difficulty, "easy", "Phones play bots on Easy");
  const code = new URL(carol.page.url()).searchParams.get("room");

  // Bob's phone opens on single player, so its arena loads and the join reloads the page.
  const bob = await openPhone((rooms) => rooms.filter((room) => room.room === code));
  await bob.page.goto(base.href);
  await bob.page.locator("#startup-overlay[data-state=ready]").waitFor();
  await bob.page.locator("#tab-multiplayer").tap();
  await bob.page.waitForFunction(
    (code) => document.querySelector(`input[name="room-choice"][value="${code}"]`)?.checked,
    code,
  );
  await expectVisible(bob.page, ["#join-room"], ["#create-room", ".room-browse"]);
  assert.equal(await bob.page.locator(".map-choice").evaluate((part) => part.inert), true);
  assert.equal(await bob.page.locator('.room-map input[name="mapMode"]').inputValue(), "harbor");
  await expectVisible(bob.page, [".open-room-note"], [".new-room-note"]);
  await bob.page.screenshot({ path: `${output}/join-setup.png` });
  await bob.page.locator("#join-room").tap();
  await until(() => carol.room.lobby?.players.length === 2, "Bob's phone joins Carol's room");
  await bob.page.waitForURL((url) => url.searchParams.get("room") === code);
  await bob.page.waitForFunction(() => !document.querySelector("#startup-overlay"), null, {
    timeout: 60000,
  });
  // Only development builds expose the room's diagnostics; the public dev site does not.
  const zoom = await bob.page.evaluate(() => window.sloppyMultiplayer?.view?.zoom ?? null);
  if (zoom !== null) {
    assert.equal(zoom, 40, "Rooms start a phone's camera as far out as single player");
  }
  await expectVisible(
    bob.page,
    [".touch-drive", "#pause", "#view-mode", ".scoreboard"],
    [".touch-aim", ".touch-fire", "#network-players"],
  );
  await bob.page.screenshot({ path: `${output}/join-arena.png` });
  await bob.page.locator("#pause").tap();
  await expectVisible(
    bob.page,
    ["#network-title", "#network-resume", "#leave-room"],
    [".network-next", ".network-help", "#network-roster"],
  );
  await bob.page.screenshot({ path: `${output}/join-menu.png` });
  await bob.page.locator("#leave-room").tap();
  await until(() => carol.room.lobby?.players.length === 1, "Bob's phone leaves the room");
  await bob.page.locator("#startup-overlay .start").waitFor();
  assert.deepEqual(errors, []);
  console.log(
    "Phone multiplayer: one-action tab, new room on the chosen map with Easy bots, busiest open room with its map shown, reload join, phone camera, controls and short menu, leave passed.",
  );
} finally {
  await browser.close();
}
