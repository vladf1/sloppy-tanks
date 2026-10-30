// First person with the mouse: V and the HUD's view button switch to it and back, the
// cursor locks while it steers and mouse travel turns the view (by the same angle per CSS
// pixel at pixel ratios 1 and 2, in Chromium and in a browser winit takes for another
// engine), the turret follows and W drives where it looks. The
// first Esc frees the cursor and the round plays on, a click on the arena takes it back
// without firing, a second Esc pauses. A death keeps the lock; Esc then frees it without
// pausing and a click on the respawn cards picks the next tank.
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import {
  clickWidget,
  collectErrors,
  launchChrome,
  nextFrame,
  openShell,
  shellDebug,
  untilState,
  untilWidget,
} from "./browser-helpers.mjs";

const out = "artifacts/performance/first-person-check";
mkdirSync(out, { recursive: true });
const browser = await launchChrome();
const errors = [];
const turn = (from, to) => Math.atan2(Math.sin(to - from), Math.cos(to - from));
const frames = async (page, count) => {
  for (let index = 0; index < count; index += 1) await nextFrame(page);
};
const debug = async (page) => (await nextFrame(page, ["debug"])).extras.debug;
// The player's own shots (`shotsFired` counts every tank's).
const shots = (debug) => {
  assert.equal(typeof debug.combatRecord.shots, "number");
  return debug.combatRecord.shots;
};
const locked = (page) =>
  page.evaluate(() => document.pointerLockElement === document.querySelector("canvas"));

/** Safari's user agent, with no `userAgentData`: winit and the shell read it as WebKit,
 * whose `movementX` winit scales by the pixel ratio (Chrome's stays in CSS pixels). */
const SAFARI =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";

/** A page at `deviceScaleFactor` (as Safari with `safari`), in a round on the Bruiser. */
async function round(deviceScaleFactor, safari = false) {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor,
    ...(safari ? { userAgent: SAFARI } : {}),
  });
  if (safari) {
    await context.addInitScript(() => delete Object.getPrototypeOf(navigator).userAgentData);
  }
  const page = await context.newPage();
  const name = `${deviceScaleFactor}x${safari ? " as Safari" : ""}`;
  collectErrors(page, errors, { consoleErrors: true, prefix: `[${name}] ` });
  await openShell(page);
  await untilState(page, (state) => state.ready, { what: "the arena ready" });
  await clickWidget(page, "BRUISER");
  await untilState(page, (state) => state.prefs.tank === "balanced" && state.ready, {
    what: "the Bruiser's arena",
  });
  await clickWidget(page, "GO!");
  await untilState(page, (state) => state.inRound && state.phase === "playing", {
    what: "the round",
  });
  return { context, page };
}

/** Enter first person with V and wait for the seat, with the cursor locked. */
async function seat(page) {
  await page.keyboard.press("KeyV");
  await untilState(page, (state) => state.firstPerson && state.extras.debug.view.inFirstPerson, {
    extras: ["debug"],
    what: "the first-person seat",
  });
  const start = Date.now();
  while (!(await locked(page))) {
    assert.ok(Date.now() - start < 5000, "first person locks the cursor");
    await nextFrame(page);
  }
}

/** The view's turn per CSS pixel of mouse travel to the right. */
async function yawPerPixel(page) {
  await page.mouse.move(640, 400);
  await frames(page, 3);
  const before = (await debug(page)).view.firstPerson.yaw;
  for (let x = 660; x <= 840; x += 20) {
    await page.mouse.move(x, 400);
    await nextFrame(page);
  }
  await frames(page, 3);
  const after = await debug(page);
  return { perPixel: turn(before, after.view.firstPerson.yaw) / 200, before, after };
}

try {
  // The turn per CSS pixel does not depend on the pixel ratio.
  const sharp = await round(2);
  await seat(sharp.page);
  const sharpTurn = (await yawPerPixel(sharp.page)).perPixel;
  await sharp.context.close();
  const safari = await round(2, true);
  assert.equal(
    await safari.page.evaluate(() => navigator.userAgentData === undefined),
    true,
    "the Safari page has no userAgentData",
  );
  await seat(safari.page);
  const safariTurn = (await yawPerPixel(safari.page)).perPixel;
  await safari.context.close();

  const { page } = await round(1);
  const shot = (name) => page.screenshot({ path: `${out}/${name}.png` });
  await page.mouse.move(800, 300);
  await frames(page, 10);
  const overheadAim = (await debug(page)).human.aim;
  await seat(page);
  let view = await debug(page);
  assert.ok(
    Math.abs(turn(overheadAim, view.view.firstPerson.yaw)) < 0.05,
    `entering keeps the turret aim: ${overheadAim} → ${view.view.firstPerson.yaw}`,
  );
  const [eyeX, eyeY, eyeZ] = view.view.camera.position;
  assert.ok(eyeY > 1 && eyeY < 3, `turret-top eye: ${eyeY}`);
  assert.ok(Math.hypot(eyeX - view.human.x, eyeZ - view.human.z) < 1.5, "the eye rides the tank");
  await untilWidget(page, "First-person view (V)");
  await shot("entered");

  // Travel to the right turns the view and turret right (lower yaw).
  const { perPixel, before, after } = await yawPerPixel(page);
  assert.ok(turn(before, after.view.firstPerson.yaw) < -0.3, `mouse right turns right`);
  for (const [name, other] of [
    ["2x", sharpTurn],
    ["2x as Safari", safariTurn],
  ]) {
    assert.ok(
      Math.abs(perPixel - other) < Math.abs(perPixel) * 0.1,
      `the turn per CSS pixel at 1x ${perPixel} and ${name} ${other}`,
    );
  }
  await frames(page, 6);
  view = await debug(page);
  assert.ok(
    Math.abs(turn(view.view.firstPerson.yaw, view.human.aim)) < 0.05,
    `the turret follows the view: ${view.view.firstPerson.yaw} vs ${view.human.aim}`,
  );

  // W drives where the turret looks, not toward the top of the screen.
  const yaw = view.view.firstPerson.yaw;
  await page.keyboard.down("KeyW");
  await page.waitForTimeout(1500);
  await page.keyboard.up("KeyW");
  const end = await debug(page);
  const moved = Math.hypot(end.human.x - view.human.x, end.human.z - view.human.z);
  const along =
    ((end.human.x - view.human.x) * Math.sin(yaw) + (end.human.z - view.human.z) * Math.cos(yaw)) /
    moved;
  assert.ok(moved > 1 && along > 0.8, `W follows the view: ${JSON.stringify({ moved, along })}`);
  await shot("driving");

  // A click with the cursor locked fires.
  let fired = shots(end);
  await page.mouse.down();
  await untilState(page, (state) => shots(state.extras.debug) > fired, {
    extras: ["debug"],
    what: "a locked click's shot",
  });
  await page.mouse.up();

  // The first Esc frees the cursor, the round plays on, and the view holds still.
  await page.keyboard.press("Escape");
  let state = await untilState(page, (state) => state.aimHint, {
    extras: ["debug"],
    what: "the aim hint",
  });
  assert.equal(state.phase, "playing", "the first Esc does not pause");
  assert.equal(await locked(page), false, "the first Esc frees the cursor");
  const still = state.extras.debug.view.firstPerson.yaw;
  await page.mouse.move(1100, 300, { steps: 5 });
  await frames(page, 4);
  assert.equal((await debug(page)).view.firstPerson.yaw, still, "a free cursor turns nothing");
  await shot("cursor-free");

  // A click on the arena takes the cursor back and does not fire.
  await page.waitForTimeout(600);
  fired = shots(await debug(page));
  await page.mouse.move(800, 450);
  await page.mouse.down();
  await frames(page, 3);
  await page.mouse.up();
  await untilState(page, (state) => !state.aimHint, { what: "the cursor taken back" });
  assert.equal(await locked(page), true, "the click locks the cursor again");
  await frames(page, 20);
  assert.equal(shots(await debug(page)), fired, "the click that takes the cursor back");

  // Esc frees it again; a second Esc, past the browser's own release, pauses.
  await page.keyboard.press("Escape");
  await untilState(page, (state) => state.aimHint, { what: "the cursor freed again" });
  await page.waitForTimeout(300);
  await page.keyboard.press("Escape");
  await untilState(page, (state) => state.phase === "paused", { what: "the pause" });
  await clickWidget(page, "RESUME");
  await untilState(page, (state) => state.phase === "playing", { what: "the resume" });
  // RESUME was a click: the cursor locks again at once.
  for (let start = Date.now(); !(await locked(page)); await nextFrame(page)) {
    assert.ok(Date.now() - start < 5000, "RESUME locks the cursor again");
  }

  // A death keeps the cursor locked and the view in the turret; the respawn needs no
  // click and comes back in first person, still locked.
  await shellDebug(page, "killHuman");
  state = await untilState(page, (state) => !state.alive, { extras: ["debug"], what: "the death" });
  assert.equal(state.extras.debug.view.inFirstPerson, true, "a wreck keeps the first-person view");
  await untilWidget(page, "BIG RIG");
  assert.equal(await locked(page), true, "a death keeps the cursor locked");
  await shot("destroyed");
  await untilState(page, (state) => state.alive && state.extras.debug.view.inFirstPerson, {
    extras: ["debug"],
    timeout: 10000,
    what: "the respawn in first person",
  });
  assert.equal(await locked(page), true, "the respawn keeps the cursor locked");

  // Esc while destroyed frees the cursor without pausing; a click picks the Big Rig.
  await shellDebug(page, "killHuman");
  await untilState(page, (state) => !state.alive, { what: "the second death" });
  await page.keyboard.press("Escape");
  state = await untilState(page, () => true, { what: "a frame" });
  assert.equal(state.phase, "playing", "Esc while destroyed does not pause");
  assert.equal(await locked(page), false, "Esc while destroyed frees the cursor");
  await shot("respawn");
  await clickWidget(page, "BIG RIG");
  state = await untilState(page, (state) => state.prefs.tank === "heavy", {
    what: "the Big Rig picked",
  });
  assert.equal(state.alive, false, "the pick waits for the respawn");
  state = await untilState(
    page,
    (state) =>
      state.alive &&
      state.extras.debug.human.kind === "heavy" &&
      state.extras.debug.view.inFirstPerson,
    { extras: ["debug"], timeout: 10000, what: "the Big Rig in first person" },
  );
  assert.equal(state.aimHint, true, "the new tank waits for a click to look");

  // The free cursor reaches the HUD: its view button goes back overhead, and in again.
  await clickWidget(page, "First-person view (V)");
  state = await untilState(page, (state) => state.extras.debug.view.camera.fov === 43, {
    extras: ["debug"],
    what: "the overhead camera",
  });
  assert.equal(state.firstPerson, false);
  assert.ok(state.extras.debug.view.camera.position[1] > 15, "the overhead camera is up high");
  assert.equal(await locked(page), false, "overhead frees the cursor");
  await shot("overhead");
  await clickWidget(page, "First-person view (V)");
  await untilState(page, (state) => state.firstPerson && state.extras.debug.view.inFirstPerson, {
    extras: ["debug"],
    what: "first person from the HUD's button",
  });
  for (let start = Date.now(); !(await locked(page)); await nextFrame(page)) {
    assert.ok(Date.now() - start < 5000, "the HUD's button locks the cursor");
  }
  await page.keyboard.press("KeyV");
  await untilState(page, (state) => !state.firstPerson, { what: "V back overhead" });

  state = await nextFrame(page);
  assert.equal(state.error, null, "renderer errors");
  assert.deepEqual(errors, []);
  console.log(
    `first-person-check: seat, look (${perPixel.toFixed(5)} rad/px at 1x and 2x), drive, Esc, retake, death and respawn pick passed`,
  );
} finally {
  await browser.close();
}
