// The phone edition in headless Chrome as a phone (a 393×852 screen, a coarse pointer,
// three device pixels a point), held upright and sideways: Battle Setup offers only the
// tank, the map, the format and the tabs, every choice on screen and taken by a tap, with
// Easy bots; GO plays with the overhead camera out at 40 m, no mine button, the top row's
// zoom keys, the drive stick and a finger on the arena firing at once; the top row's
// pause, END BATTLE, the battle report and BATTLE SETUP. `?phone` asks for the edition
// on a desktop page too. Real multi-touch through CDP touch events (`fingers`).
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import {
  clickWidget,
  collectErrors,
  fingers,
  launchChrome,
  nextFrame,
  openShell,
  tapWidget,
  untilState,
} from "./browser-helpers.mjs";

const out = "artifacts/performance/phone-check";
mkdirSync(out, { recursive: true });
const browser = await launchChrome();
const errors = [];
const SCREEN = { width: 393, height: 852 };
/** Everything the phone's Battle Setup offers. */
const SETUP = [
  "SINGLE PLAYER",
  "MULTIPLAYER",
  "SKIPPER",
  "BRUISER",
  "BIG RIG",
  "Pine Village",
  "Harbor Havoc",
  "Dusty Dig",
  "Team Battle",
  "Solo Assault",
  "GO!",
];
/** The phone's own camera distance (`PHONE_ZOOM`) and the zoom keys' step. */
const PHONE_ZOOM = 40;
const ZOOM_STEP = 2;
const middle = (area) => [area.x + area.width / 2, area.y + area.height / 2];
const shots = (debug) => debug.combatRecord.shots;
const distance = (a, b) => Math.hypot(a.human.x - b.human.x, a.human.z - b.human.z);
const debug = async (page) => (await nextFrame(page, ["debug"])).extras.debug;
const settled = async (page, ms) => {
  await page.waitForTimeout(ms);
  return debug(page);
};
const widgets = (page) => page.evaluate(() => window.sloppyShell.widgets());

/** Battle Setup shows exactly the phone's choices, each inside `viewport`. */
async function phoneSetup(page, viewport, what) {
  await untilState(page, (state) => state.ready, { what: `${what}: the arena ready` });
  // A sheet that has just opened lists its widgets a frame later.
  let shown = await widgets(page);
  for (let frame = 0; frame < 30 && shown.length < SETUP.length; frame += 1) {
    await nextFrame(page);
    shown = await widgets(page);
  }
  assert.deepEqual(
    shown.map((widget) => widget.label).sort(),
    [...SETUP].sort(),
    `${what}: Battle Setup's choices`,
  );
  for (const widget of shown) {
    const inside =
      widget.x >= 0 &&
      widget.y >= 0 &&
      widget.x + widget.width <= viewport.width + 0.5 &&
      widget.y + widget.height <= viewport.height + 0.5;
    assert.ok(inside, `${what}: ${widget.label} on screen: ${JSON.stringify(widget)}`);
  }
}

/** One phone held `viewport`'s way up, from Battle Setup to the report and back. */
async function playPhone(name, viewport, [tank, tankKind], [map, mapId]) {
  const context = await browser.newContext({
    viewport,
    screen: SCREEN,
    deviceScaleFactor: 3,
    hasTouch: true,
    isMobile: true,
  });
  const page = await context.newPage();
  collectErrors(page, errors, { consoleErrors: true, prefix: `[${name}] ` });
  const shot = (step) => page.screenshot({ path: `${out}/${name}-${step}.png` });
  await openShell(page);
  const touch = await fingers(page);
  await phoneSetup(page, viewport, name);
  let state = await nextFrame(page, ["debug"]);
  assert.equal(state.touchScreen, true, `${name}: a phone plays by touch`);
  assert.equal(state.extras.debug.difficulty, "easy", `${name}: a phone's bots are Easy`);
  await shot("setup");

  // Every choice takes a tap.
  await tapWidget(page, touch, tank);
  await tapWidget(page, touch, map);
  await tapWidget(page, touch, "Solo Assault");
  await untilState(
    page,
    (state) =>
      state.prefs.tank === tankKind && state.prefs.map === mapId && state.prefs.gameMode === "solo",
    { what: `${name}: the tapped choices` },
  );
  await tapWidget(page, touch, "Team Battle");
  await untilState(page, (state) => state.prefs.gameMode === "team" && state.ready, {
    timeout: 60000,
    what: `${name}: Team Battle on the chosen arena`,
  });
  await phoneSetup(page, viewport, `${name} after the choices`);

  // GO: the phone's camera, no mine button, the top row's keys.
  await tapWidget(page, touch, "GO!");
  state = await untilState(page, (state) => state.inRound && state.touchControls, {
    extras: ["debug"],
    what: `${name}: the round's touch controls`,
  });
  assert.equal(state.extras.debug.mapMode, mapId, `${name}: the chosen arena`);
  assert.equal(state.extras.debug.human.kind, tankKind, `${name}: the chosen tank`);
  assert.equal(state.extras.debug.view.zoom, PHONE_ZOOM, `${name}: the phone's camera`);
  const layout = state.touchLayout;
  assert.equal(layout.mine, null, `${name}: a phone has no mine button`);
  // An egui area lists its widgets from its second frame on.
  await page.waitForTimeout(500);
  const labels = (await widgets(page)).map((widget) => widget.label).sort();
  assert.deepEqual(labels, ["Pause", "Switch view"], `${name}: the top row's buttons`);
  await shot("round");

  // The top row's zoom keys step the camera in and out again.
  await touch.tap(1, ...middle(layout.zoomIn));
  await untilState(page, (state) => state.extras.debug.view.zoom === PHONE_ZOOM - ZOOM_STEP, {
    extras: ["debug"],
    what: `${name}: the zoom-in key`,
  });
  await touch.tap(2, ...middle(layout.zoomOut));
  await untilState(page, (state) => state.extras.debug.view.zoom === PHONE_ZOOM, {
    extras: ["debug"],
    what: `${name}: the zoom-out key`,
  });

  // A thumb drives; a finger on the arena fires while the thumb drives back.
  const stick = middle(layout.stick);
  let before = await debug(page);
  await touch.start(3, ...stick);
  await touch.move(3, stick[0], stick[1] - 50);
  let now = await settled(page, 800);
  assert.ok(distance(before, now) > 1, `${name}: the stick drives`);
  before = now;
  const arena = [viewport.width * 0.7, viewport.height * 0.4];
  await touch.start(4, ...arena);
  await touch.move(3, stick[0], stick[1] + 50);
  now = await settled(page, 1000);
  assert.ok(shots(now) > shots(before), `${name}: the arena finger fires`);
  assert.ok(distance(before, now) > 1, `${name}: the stick drives under a second finger`);
  await shot("drive-and-fire");
  await touch.end(4);
  await touch.end(3);
  state = await nextFrame(page);
  assert.deepEqual([state.stickPush, state.arenaFinger], [null, false], `${name}: both lifted`);

  // The top row's pause, END BATTLE, the battle report and BATTLE SETUP.
  await tapWidget(page, touch, "Pause");
  await untilState(page, (state) => state.phase === "paused", { what: `${name}: the pause` });
  await tapWidget(page, touch, "END BATTLE");
  await untilState(page, (state) => state.phase === "results", { what: `${name}: the report` });
  await page.waitForTimeout(800);
  await shot("report");
  await tapWidget(page, touch, "BATTLE SETUP");
  await untilState(page, (state) => !state.inRound, { what: `${name}: Battle Setup again` });
  await phoneSetup(page, viewport, `${name} back from the report`);
  assert.equal((await nextFrame(page)).error, null, `${name}: renderer errors`);
  await context.close();
}

try {
  await playPhone("upright", SCREEN, ["BIG RIG", "heavy"], ["Harbor Havoc", "harbor"]);
  await playPhone(
    "sideways",
    { width: SCREEN.height, height: SCREEN.width },
    ["SKIPPER", "scout"],
    ["Dusty Dig", "quarry"],
  );

  // `?phone` gives a desktop page the phone edition.
  {
    const viewport = { width: 1280, height: 800 };
    const context = await browser.newContext({ viewport, screen: viewport, deviceScaleFactor: 1 });
    const page = await context.newPage();
    collectErrors(page, errors, { consoleErrors: true, prefix: "[?phone] " });
    await openShell(page, "phone");
    await phoneSetup(page, viewport, "?phone");
    assert.equal((await nextFrame(page)).touchScreen, true, "?phone plays by touch");
    await page.screenshot({ path: `${out}/forced-setup.png` });
    await clickWidget(page, "GO!");
    const state = await untilState(page, (state) => state.inRound && state.touchControls, {
      what: "?phone: the round",
    });
    assert.equal(state.touchLayout.mine, null, "?phone: the phone's controls");
    await page.screenshot({ path: `${out}/forced-round.png` });
    await context.close();
  }

  assert.deepEqual(errors, []);
  console.log(
    "phone-check: upright, sideways and ?phone Battle Setup, play, pause and report passed",
  );
} finally {
  await browser.close();
}
