// Play the page's main path with real input: Battle Setup's choices, GO, the drive
// keys, firing, the wheel, the camera, pause, Settings, the battle report and back to
// Battle Setup, and a second round on the same arena with no bodies left behind. The
// page is one egui canvas, so the check finds widgets through the shell's automation
// hook (`openShell`, `clickWidget`) and clicks their coordinates with the real mouse.
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import {
  clickWidget,
  launchGame,
  nextFrame,
  openShell,
  shellDebug,
  untilState,
} from "./browser-helpers.mjs";

const out = "artifacts/performance/shell-check";
mkdirSync(out, { recursive: true });
const { browser, page, errors } = await launchGame({
  viewport: { width: 1280, height: 800 },
  consoleErrors: true,
});
const shot = (name) => page.screenshot({ path: `${out}/${name}.png` });
const distance = (a, b) => Math.hypot(a.x - b.x, a.z - b.z);

try {
  await openShell(page);
  await untilState(page, (state) => state.ready, { what: "the arena ready" });
  await shot("battle-setup");

  // Battle Setup's choices reach the saved record and the arena.
  await clickWidget(page, "Harbor Havoc");
  await clickWidget(page, "SKIPPER");
  await clickWidget(page, "HARD");
  let state = await untilState(
    page,
    (state) => state.prefs.map === "harbor" && state.prefs.tank === "scout" && state.ready,
    { what: "Harbor Havoc, the Skipper and the arena ready" },
  );
  assert.equal(state.prefs.difficulty, "hard");

  await clickWidget(page, "GO!");
  state = await untilState(page, (state) => state.inRound && state.phase === "playing", {
    extras: ["debug"],
    what: "the round",
  });
  assert.equal(state.extras.debug.mapMode, "harbor");
  assert.equal(state.extras.debug.human.kind, "scout");
  const bodies = state.extras.debug.bodies;
  await shot("round");

  // The drive keys move the tank.
  const start = state.extras.debug.human;
  await page.keyboard.down("KeyW");
  await page.waitForTimeout(700);
  await page.keyboard.up("KeyW");
  state = await nextFrame(page, ["debug"]);
  assert.ok(
    distance(start, state.extras.debug.human) > 1,
    `W drove ${distance(start, state.extras.debug.human)} m`,
  );

  // A held left button fires at the pointer.
  const fired = state.extras.debug.shotsFired;
  await page.mouse.move(900, 300);
  await page.mouse.down();
  await page.waitForTimeout(400);
  await page.mouse.up();
  state = await untilState(page, (state) => state.extras.debug.shotsFired > fired, {
    extras: ["debug"],
    what: "a shot",
  });

  // The wheel zooms; V switches to first person and back.
  const zoom = state.extras.debug.view.zoom;
  await page.mouse.wheel(0, 400);
  state = await untilState(page, (state) => state.extras.debug.view.zoom !== zoom, {
    extras: ["debug"],
    what: "the wheel's zoom",
  });
  await page.keyboard.press("KeyV");
  await untilState(page, (state) => state.firstPerson, { what: "first person" });
  await page.keyboard.press("KeyV");
  await untilState(page, (state) => !state.firstPerson, { what: "the overhead camera" });

  // Esc pauses; RESUME carries on.
  await page.keyboard.press("Escape");
  await untilState(page, (state) => state.phase === "paused", { what: "the pause" });
  await shot("paused");
  await clickWidget(page, "RESUME");
  await untilState(page, (state) => state.phase === "playing", { what: "the resume" });

  // Settings open from the pause menu and close without saving.
  await page.keyboard.press("Escape");
  await clickWidget(page, "SETTINGS");
  await untilState(page, (state) => state.settings, { what: "Settings" });
  await shot("settings");
  await clickWidget(page, "Close");
  await untilState(page, (state) => !state.settings, { what: "Settings closed" });

  // END BATTLE shows the battle report; BATTLE SETUP goes back.
  await clickWidget(page, "END BATTLE");
  await untilState(page, (state) => state.phase === "results", { what: "the battle report" });
  await shot("report");
  await clickWidget(page, "BATTLE SETUP");
  await untilState(page, (state) => !state.inRound && state.ready, {
    what: "Battle Setup ready again",
  });

  // A second round starts on the same arena with the same bodies.
  await clickWidget(page, "GO!");
  state = await untilState(page, (state) => state.inRound && state.phase === "playing", {
    extras: ["debug"],
    what: "the second round",
  });
  assert.equal(state.extras.debug.bodies, bodies, "bodies after a reset");
  await shellDebug(page, "killHuman");
  await untilState(page, (state) => !state.alive, { what: "the death" });
  await shot("destroyed");

  state = await nextFrame(page);
  assert.equal(state.error, null, "renderer errors");
  assert.deepEqual(errors, []);
  console.log("shell-check: Battle Setup, controls, pause, Settings, report and reset passed");
} finally {
  await browser.close();
}
