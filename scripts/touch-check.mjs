// Touch play in headless Chrome with real multi-touch (CDP touch events). A tablet
// (1180×820, a coarse pointer, pixel ratio 2): GO by a finger, the stick drives, a finger
// on the arena fires, both at once and either one lifting leaves the other at work, the
// mine button drops a mine, the zoom keys step the camera, the toolbar's pause pauses and
// hides the controls. Settings' Touch controls: Off is saved, turns the controls off and
// holds over a touch and a reload; Auto brings them back. A desktop with a touch screen
// (a fine pointer): keyboard and mouse until a finger touches the page on Auto, the mouse
// still firing after; Off holds over a touch; On shows the controls with no touch.
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

const out = "artifacts/performance/touch-check";
mkdirSync(out, { recursive: true });
const browser = await launchChrome();
const errors = [];
const TABLET = { width: 1180, height: 820 };
const DESKTOP = { width: 1280, height: 800 };
const middle = (area) => [area.x + area.width / 2, area.y + area.height / 2];
const shots = (debug) => debug.combatRecord.shots;
const at = (debug) => ({ x: debug.human.x, z: debug.human.z });
const distance = (a, b) => Math.hypot(a.x - b.x, a.z - b.z);
const debug = async (page) => (await nextFrame(page, ["debug"])).extras.debug;
const settled = async (page, ms) => {
  await page.waitForTimeout(ms);
  return debug(page);
};

/** A page on `device`'s screen; its errors go to `errors` under `name`. */
async function open(name, device, query = "") {
  const context = await browser.newContext(device);
  const page = await context.newPage();
  collectErrors(page, errors, { consoleErrors: true, prefix: `[${name}] ` });
  await openShell(page, query);
  await untilState(page, (state) => state.ready, { what: `${name}: the arena ready` });
  return { context, page, shot: (step) => page.screenshot({ path: `${out}/${name}-${step}.png` }) };
}

/** Settings' Touch controls set to `choice` and saved, by finger or by mouse. */
async function chooseTouch(page, choice, touch) {
  const press = (label) => (touch ? tapWidget(page, touch, label) : clickWidget(page, label));
  await press("Settings");
  await untilState(page, (state) => state.settings, { what: "Settings" });
  await press(`touch controls: ${choice}`);
  await press("SAVE");
  return untilState(
    page,
    (state) => !state.settings && state.prefs.touch === choice.toLowerCase(),
    {
      what: `Touch controls ${choice} saved`,
    },
  );
}

const tabletDevice = {
  viewport: TABLET,
  screen: TABLET,
  deviceScaleFactor: 2,
  hasTouch: true,
  isMobile: true,
};

try {
  // A tablet plays by touch from the start.
  {
    const { context, page, shot } = await open("tablet", tabletDevice);
    const touch = await fingers(page);
    let state = await nextFrame(page);
    assert.equal(state.touchScreen, true, "a coarse pointer plays by touch");
    await shot("setup");
    await tapWidget(page, touch, "GO!");
    state = await untilState(page, (state) => state.inRound && state.touchControls, {
      what: "the round's touch controls",
    });
    const layout = state.touchLayout;
    const stick = middle(layout.stick);
    await page.waitForTimeout(500);

    // The stick drives.
    let before = await debug(page);
    await touch.start(1, ...stick);
    await touch.move(1, stick[0], stick[1] - 60);
    state = await nextFrame(page);
    assert.ok(state.stickPush?.[1] < -0.9, `the stick pushes up: ${state.stickPush}`);
    let now = await settled(page, 800);
    assert.ok(distance(at(before), at(now)) > 1, "the stick drives");
    assert.equal(shots(now), shots(before), "the stick alone fires nothing");

    // A second finger on the arena fires while the stick, pulled back, drives the tank
    // back the way it came (open ground, whatever the spawn).
    before = now;
    await touch.start(2, 900, 300);
    await touch.move(1, stick[0], stick[1] + 60);
    state = await nextFrame(page);
    assert.equal(state.arenaFinger, true, "the arena has a finger");
    assert.ok(state.stickPush?.[1] > 0.9, `the stick pushes down: ${state.stickPush}`);
    now = await settled(page, 1000);
    assert.ok(shots(now) > shots(before), "an arena finger fires");
    assert.ok(distance(at(before), at(now)) > 1, "the stick drives on under a second finger");
    await shot("drive-and-fire");

    // Lifting the arena finger stops the fire and leaves the stick driving.
    await touch.end(2);
    state = await nextFrame(page);
    assert.equal(state.arenaFinger, false, "the arena finger lifted");
    assert.ok(state.stickPush?.[1] > 0.9, "the stick keeps its push after the other lifts");
    before = await settled(page, 300);
    now = await settled(page, 700);
    assert.equal(shots(now), shots(before), "the lifted finger fires no more");
    assert.ok(distance(at(before), at(now)) > 0.3, "the stick drives on after the other lifts");
    await touch.end(1);
    state = await nextFrame(page);
    assert.equal(state.stickPush, null, "the stick springs back");
    before = await settled(page, 600);
    now = await settled(page, 800);
    assert.ok(distance(at(before), at(now)) < 0.3, "the released stick stops the tank");

    // The stick and the arena again; the stick lifts first and the fire goes on.
    await touch.start(3, ...stick);
    await touch.start(4, 880, 320);
    await touch.end(3);
    state = await nextFrame(page);
    assert.equal(state.stickPush, null, "the stick lifted");
    assert.equal(state.arenaFinger, true, "the arena finger stays after the stick lifts");
    before = await settled(page, 300);
    now = await settled(page, 1200);
    assert.ok(shots(now) > shots(before), "the arena finger fires on after the stick lifts");
    await touch.end(4);
    assert.equal((await nextFrame(page)).arenaFinger, false);

    // The mine button drops a mine.
    assert.ok(layout.mine, "a tablet has a mine button");
    await touch.tap(4, ...layout.mine);
    await untilState(page, (state) => state.extras.debug.human.mineCooldown > 0, {
      extras: ["debug"],
      what: "the mine button's mine",
    });

    // The zoom keys step the camera in and out.
    const zoom = (await debug(page)).view.zoom;
    await touch.tap(5, ...middle(layout.zoomIn));
    state = await untilState(page, (state) => state.extras.debug.view.zoom < zoom, {
      extras: ["debug"],
      what: "the zoom-in key",
    });
    const closer = state.extras.debug.view.zoom;
    await touch.tap(6, ...middle(layout.zoomOut));
    await untilState(page, (state) => state.extras.debug.view.zoom > closer, {
      extras: ["debug"],
      what: "the zoom-out key",
    });

    // The toolbar's pause pauses and hides the controls; RESUME brings them back.
    await tapWidget(page, touch, "Pause (Esc)");
    state = await untilState(page, (state) => state.phase === "paused", { what: "the pause" });
    await nextFrame(page);
    assert.equal((await nextFrame(page)).touchControls, false, "paused, the controls hide");
    await shot("paused");
    await tapWidget(page, touch, "RESUME");
    await untilState(page, (state) => state.phase === "playing" && state.touchControls, {
      what: "the resume",
    });

    // Settings' Touch controls Off: saved, the controls go, and a touch does not bring
    // them back.
    state = await chooseTouch(page, "Off", touch);
    await nextFrame(page);
    assert.equal((await nextFrame(page)).touchScreen, false, "Off turns touch play off");
    await touch.tap(7, 900, 300);
    state = await nextFrame(page);
    assert.equal(state.touchScreen, false, "Off holds over a touch");
    assert.equal(state.touchControls, false);
    await shot("touch-off");
    // Off holds over a reload too.
    await openShell(page);
    await untilState(page, (state) => state.ready, { what: "the reloaded arena" });
    const reloaded = await fingers(page);
    await reloaded.tap(1, 900, 300);
    state = await nextFrame(page);
    assert.equal(state.prefs.touch, "off", "Off is kept");
    assert.equal(state.touchScreen, false, "Off holds over a reload and a touch");
    // Auto follows the screen again.
    await chooseTouch(page, "Auto", reloaded);
    assert.equal((await nextFrame(page)).touchScreen, true, "Auto plays by touch again");
    await context.close();
  }

  // A desktop with a touch screen: keyboard and mouse until a finger touches the page.
  // Chrome's touch emulation also makes the pointer coarse, so it starts after the page
  // has read its fine one, as a touch laptop's primary pointer stays the touchpad.
  {
    const device = { viewport: DESKTOP, screen: DESKTOP, deviceScaleFactor: 1 };
    const { context, page, shot } = await open("touch-laptop", device);
    const emulation = await context.newCDPSession(page);
    await emulation.send("Emulation.setTouchEmulationEnabled", {
      enabled: true,
      maxTouchPoints: 5,
    });
    const touch = await fingers(page);
    let state = await nextFrame(page);
    assert.equal(state.touchScreen, false, "a fine pointer plays with keyboard and mouse");
    await clickWidget(page, "GO!");
    state = await untilState(page, (state) => state.inRound && state.phase === "playing", {
      what: "the round",
    });
    assert.equal(state.touchControls, false, "no touch controls before a touch");
    await touch.tap(1, 900, 300);
    state = await untilState(page, (state) => state.touchControls, {
      what: "the touch controls after the first touch",
    });
    assert.equal(state.touchScreen, true, "Auto plays by touch from the first touch");
    await shot("after-first-touch");
    // The mouse still fires.
    const before = shots(await settled(page, 1500));
    await page.mouse.move(900, 300);
    await page.mouse.down();
    await untilState(page, (state) => shots(state.extras.debug) > before, {
      extras: ["debug"],
      what: "a mouse shot on a touch laptop",
    });
    await page.mouse.up();
    // Off holds over a touch; On shows the controls without one.
    await chooseTouch(page, "Off");
    await touch.tap(2, 900, 300);
    state = await nextFrame(page);
    assert.equal(state.touchScreen, false, "Off holds over a touch");
    await chooseTouch(page, "On");
    state = await untilState(page, (state) => state.touchControls, { what: "On's controls" });
    assert.equal(state.touchScreen, true);
    await shot("touch-on");
    await context.close();
  }

  assert.deepEqual(errors, []);
  console.log(
    "touch-check: tablet stick, fire, both at once, mine, zoom, pause and Touch controls passed",
  );
} finally {
  await browser.close();
}
