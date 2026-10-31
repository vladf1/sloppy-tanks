// Play the page's menu before its engine has arrived. The page's binary comes in two
// halves (`scripts/split-wasm.mjs`): Battle Setup runs on the menu half while the engine
// half downloads, and any call from the menu into an engine function the engine half
// has not delivered yet throws (the shell then stops, naming the placeholder:
// `src/generated/shell/split/placeholders` names the function). This check holds the
// engine half back, plays Battle Setup's choices, Settings, the Multiplayer tab and GO
// on the menu alone, then lets the engine in: the window must pass to it with the same
// team and choices, and GO's round must start by itself.
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { clickWidget, launchGame, nextFrame, openShell, untilState } from "./browser-helpers.mjs";

const out = "artifacts/performance/split-check";
mkdirSync(out, { recursive: true });
const { browser, page, errors } = await launchGame({
  viewport: { width: 1280, height: 800 },
  consoleErrors: true,
});
const shot = (name) => page.screenshot({ path: `${out}/${name}.png` });
// The Multiplayer tab asks a room directory that need not run here.
const pageErrors = () => errors.filter((error) => !/Failed to load resource/.test(error));

let release;
const engineHeld = new Promise((resolve) => (release = resolve));
let engineAsked = false;
// The binary itself: the dev server also answers the page's `?url` import of it.
const engineHalf = (url) => /_engine(-[\w-]+)?\.wasm$/.test(url.pathname) && !url.search;
await page.route(engineHalf, async (route) => {
  engineAsked = true;
  await engineHeld;
  await route.continue();
});

try {
  let state = await openShell(page);
  assert.equal(state.engine, "menu", "Battle Setup shows before the engine arrives");
  assert.equal(state.ready, false);
  const team = state.team;
  assert.match(team, /^(blue|red)$/);
  await shot("menu");

  // Battle Setup's choices are kept on the menu alone.
  await clickWidget(page, "Harbor Havoc");
  await clickWidget(page, "SKIPPER");
  await clickWidget(page, "Hard");
  state = await untilState(
    page,
    (state) =>
      state.prefs.map === "harbor" &&
      state.prefs.tank === "scout" &&
      state.prefs.difficulty === "hard",
    { what: "Harbor Havoc, the Skipper and Hard on the menu" },
  );
  assert.equal(state.engine, "menu");

  // Settings open and close.
  await clickWidget(page, "Settings");
  await untilState(page, (state) => state.settings, { what: "Settings on the menu" });
  await shot("settings");
  await clickWidget(page, "Close");
  await untilState(page, (state) => !state.settings, { what: "Settings closed" });

  // The Multiplayer tab lists rooms (none here) and comes back.
  await clickWidget(page, "MULTIPLAYER");
  await untilState(page, (state) => state.multiplayerTab, { what: "the Multiplayer tab" });
  await page.waitForTimeout(1500);
  await shot("multiplayer");
  await clickWidget(page, "SINGLE PLAYER");
  await untilState(page, (state) => !state.multiplayerTab, { what: "Single Player again" });

  // GO waits for the engine.
  await clickWidget(page, "GO!");
  state = await untilState(page, (state) => state.status === "Preparing your arena…", {
    what: "GO waiting for the engine",
  });
  assert.equal(state.engine, "menu", "the engine is still held back");
  assert.ok(engineAsked, "the page asked for the engine half while the menu showed");
  assert.deepEqual(pageErrors(), [], "the menu reached no engine code");

  // The engine arrives: the window passes to it, and GO's round starts.
  release();
  state = await untilState(page, (state) => state.inRound && state.phase === "playing", {
    extras: ["debug"],
    timeout: 60000,
    what: "GO's round once the engine arrived",
  });
  assert.equal(state.engine, "single");
  assert.equal(state.team, team, "the player's team is the one Battle Setup showed");
  assert.equal(state.extras.debug.mapMode, "harbor");
  assert.equal(state.extras.debug.human.kind, "scout");
  await shot("round");

  state = await nextFrame(page);
  assert.equal(state.error, null, "renderer errors");
  assert.deepEqual(pageErrors(), []);
  console.log("split-check: Battle Setup, Settings, Multiplayer and GO on the menu alone passed");
} finally {
  release();
  await browser.close();
}
