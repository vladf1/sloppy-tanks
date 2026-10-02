// Returning players keep their choices through reloads and between local/online play.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chooseMap, chosenMap, gameUrl, launchGame, startRound } from "./browser-helpers.mjs";
import { click, waitForRoomBrowser } from "./multiplayer-helpers.mjs";

const output = "artifacts/performance/player-ux";
await mkdir(output, { recursive: true });
const { browser, page, errors } = await launchGame({
  viewport: { width: 1440, height: 900 },
  consoleErrors: true,
});
page.setDefaultTimeout(60000);
const base = new URL(gameUrl);
if (process.env.SLOPPY_SERVER) base.searchParams.set("server", process.env.SLOPPY_SERVER);
const camera = () => page.evaluate(() => Array.from(window.sloppy.game.camera_preferences()));
const netCamera = () =>
  page.evaluate(() => Array.from(window.sloppyMultiplayer.game.camera_preferences()));
const saved = () =>
  page.evaluate(() =>
    Object.fromEntries(
      ["tank", "game-mode", "camera", "zoom", "map", "difficulty"].map((key) => [
        key,
        localStorage.getItem("sloppy-" + key),
      ]),
    ),
  );
const observations = {};
async function start() {
  await startRound(page);
  // Keep screenshots and preference checks independent of a bot's next shot.
  await page.evaluate(() => window.engine.setHuman({ protection: 600 }));
}
async function zoom() {
  await page.mouse.move(720, 400);
  await page.keyboard.down("Shift");
  await page.mouse.wheel(0, 150);
  await page.keyboard.up("Shift");
}
async function playingOnline() {
  await page.waitForFunction(() => {
    const game = window.sloppyMultiplayer;
    return game?.connection.connected && game.view && game.control?.driver === "human";
  });
  await page.waitForFunction(() => !document.querySelector("#startup-overlay"));
}
try {
  await page.goto(base.href);
  await click(page, '[data-kind="heavy"]');
  await page.locator('input[name="gameMode"][value="solo"]').check();
  await page.locator('input[name="difficulty"][value="easy"]').check();
  await chooseMap(page, "quarry");
  await page.reload();
  assert.equal(await page.locator('[data-kind="heavy"]').getAttribute("aria-pressed"), "true");
  assert.equal(await page.locator('input[name="gameMode"][value="solo"]').isChecked(), true);
  assert.equal(await page.locator('input[name="difficulty"][value="easy"]').isChecked(), true);
  assert.equal(await chosenMap(page), "quarry");
  await page.waitForFunction(
    () => document.querySelector("#startup-overlay")?.dataset.state === "ready",
  );
  await page.screenshot({ path: `${output}/remembered-setup.png` });
  await start();
  const original = await camera();
  await zoom();
  await page.waitForFunction(
    (before) => window.sloppy.game.camera_preferences()[1] !== before,
    original[1],
  );
  const zoomed = await camera();
  await page.keyboard.press("v");
  await page.waitForFunction(() => window.sloppy.view.inFirstPerson);
  observations.savedSingle = await saved();
  assert.equal(observations.savedSingle.camera, "first-person");
  assert.equal(Number(observations.savedSingle.zoom), zoomed[1]);
  await page.reload();
  await start();
  await page.waitForFunction(() => window.sloppy.view.inFirstPerson);
  assert.deepEqual(await camera(), [1, zoomed[1]], "camera and zoom survive a fresh engine");
  await page.screenshot({ path: `${output}/remembered-first-person.png` });
  // A temporary death must not become the preference for future rounds.
  await page.evaluate(() => window.sloppy.killHuman());
  assert.equal((await saved()).camera, "first-person");
  await page.reload();
  await start();
  await page.waitForFunction(() => window.sloppy.view.inFirstPerson);
  await page.keyboard.press("v");
  await page.waitForFunction(() => window.sloppy.view.camera.fov === 43);
  assert.deepEqual(await camera(), [0, zoomed[1]]);
  await page.screenshot({ path: `${output}/remembered-overhead-zoom.png` });
  await page.keyboard.press("Escape");
  await click(page, "#end-battle");
  await click(page, "#play-again");
  await page.waitForFunction(() => window.sloppy.sim.match.phase === "playing");
  assert.deepEqual(await camera(), [0, zoomed[1]], "Play Again keeps preferred overhead zoom");
  await page.keyboard.press("v");
  await page.waitForFunction(() => window.sloppy.view.inFirstPerson);

  const online = new URL(base);
  online.searchParams.set("multiplayer", "");
  await page.goto(online.href);
  await waitForRoomBrowser(page);
  assert.equal(await page.locator('[data-kind="heavy"]').getAttribute("aria-pressed"), "true");
  await page.locator("#player-name").fill("Returning player");
  await click(page, "#create-room");
  await playingOnline();
  assert.deepEqual(await netCamera(), [1, zoomed[1]], "room inherits the camera preference");
  await page.keyboard.press("v");
  await page.waitForFunction(() => window.sloppyMultiplayer.game.camera_preferences()[0] === 0);
  await zoom();
  await page.waitForFunction(
    (before) => window.sloppyMultiplayer.game.camera_preferences()[1] !== before,
    zoomed[1],
  );
  const onlineCamera = await netCamera();
  observations.savedOnline = await saved();
  assert.equal(observations.savedOnline.camera, "overhead");
  assert.equal(Number(observations.savedOnline.zoom), onlineCamera[1]);
  // Empty rooms are hidden during reconnect grace. A friend keeps this room listed.
  const friend = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  friend.on("pageerror", (error) => errors.push(error.message));
  await friend.goto(page.url());
  await friend.locator("#join-room:enabled").waitFor();
  await friend.locator("#player-name").fill("Friend");
  await click(friend, "#join-room");
  await friend.waitForFunction(() => window.sloppyMultiplayer?.connection.connected);
  await page.reload();
  await click(page, "#join-room");
  await playingOnline();
  assert.deepEqual(await netCamera(), onlineCamera, "room reload restores its camera");
  await page.goto(base.href);
  assert.equal(
    await page.locator('input[name="gameMode"][value="solo"]').isChecked(),
    true,
    "online team battles do not overwrite the single-player mode",
  );
  await start();
  assert.deepEqual(await camera(), onlineCamera, "single player inherits the latest online camera");
  observations.final = await saved();
  assert.equal(await page.evaluate(() => window.sloppy.error()), null);
  const extraLevels = new URL(base);
  extraLevels.searchParams.set("debug", "");
  await page.goto(extraLevels.href);
  await chooseMap(page, "stress-test");
  assert.equal(await page.locator('input[name="gameMode"][value="team"]').isChecked(), true);
  assert.equal((await saved())["game-mode"], "solo", "extra levels keep the saved standard mode");
  await chooseMap(page, "harbor");
  assert.equal(
    await page.locator('input[name="gameMode"][value="solo"]').isChecked(),
    true,
    "returning to a standard map restores Solo Assault immediately",
  );
  assert.deepEqual(errors, []);
  await writeFile(`${output}/preferences.json`, JSON.stringify({ observations, errors }, null, 2));
  console.log(
    "Preferences passed: tank/mode/map/difficulty reload, view/zoom reload, death, Play Again, and local/online round trips.",
  );
} finally {
  await browser.close();
}
