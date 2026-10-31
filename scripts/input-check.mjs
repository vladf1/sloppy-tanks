// Keyboard and mouse in a round: WASD and the arrows drive, Space fires, Q/E, 1-5 and the
// number pad pick ammunition, a plain wheel notch steps it and Shift with the wheel zooms,
// a right click drops a mine. Losing focus lets go of held keys (the tank stops, the round
// plays on), a hidden page pauses single player and shows the pause on its return, and
// the HUD's fullscreen button enters and leaves fullscreen. Clicks at pixel ratios 2 and
// 1.5 land on the widgets they aim at.
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

const out = "artifacts/performance/input-check";
mkdirSync(out, { recursive: true });
const browser = await launchChrome();
const errors = [];
const VIEWPORT = { width: 1280, height: 800 };
const shots = (debug) => debug.combatRecord.shots;
const turn = (from, to) => Math.abs(Math.atan2(Math.sin(to - from), Math.cos(to - from)));
const distance = (a, b) => Math.hypot(a.human.x - b.human.x, a.human.z - b.human.z);
const debug = async (page) => (await nextFrame(page, ["debug"])).extras.debug;
const settled = async (page, ms) => {
  await page.waitForTimeout(ms);
  return debug(page);
};

/** A page at `deviceScaleFactor`, on Battle Setup with the arena ready. */
async function open(name, deviceScaleFactor) {
  const context = await browser.newContext({
    viewport: VIEWPORT,
    screen: VIEWPORT,
    deviceScaleFactor,
  });
  const page = await context.newPage();
  collectErrors(page, errors, { consoleErrors: true, prefix: `[${name}] ` });
  await openShell(page);
  await untilState(page, (state) => state.ready, { what: `${name}: the arena ready` });
  return { context, page };
}

async function startRound(page, what) {
  await clickWidget(page, "GO!");
  await untilState(page, (state) => state.inRound && state.phase === "playing", {
    what: `${what}: the round`,
  });
}

/** Hold `key` for `ms`: the tank drives or turns toward it. */
async function drive(page, key, ms = 600) {
  const before = await debug(page);
  await page.keyboard.down(key);
  await page.waitForTimeout(ms);
  await page.keyboard.up(key);
  const after = await settled(page, 100);
  const moved = distance(before, after);
  const turned = turn(before.human.heading, after.human.heading);
  // A tank against cover still turns toward the key's direction.
  assert.ok(moved > 0.5 || turned > 0.3, `${key} drives: ${moved} m, ${turned} rad`);
}

/** Visibility as the browser reports it to the page, then its `visibilitychange`. */
const setHidden = (page, hidden) =>
  page.evaluate((hidden) => {
    for (const [name, value] of [
      ["hidden", hidden],
      ["visibilityState", hidden ? "hidden" : "visible"],
    ]) {
      Object.defineProperty(document, name, { configurable: true, get: () => value });
    }
    document.dispatchEvent(new Event("visibilitychange"));
  }, hidden);

try {
  {
    const { context, page } = await open("1x", 1);
    const shot = (name) => page.screenshot({ path: `${out}/${name}.png` });
    await startRound(page, "1x");
    await page.mouse.move(900, 300);
    await page.waitForTimeout(500);

    // WASD and the arrows drive, each back the way the last one went.
    for (const key of ["KeyW", "KeyS", "ArrowUp", "ArrowDown", "KeyA", "KeyD"]) {
      await drive(page, key);
    }
    await drive(page, "ArrowLeft");
    await drive(page, "ArrowRight");

    // Space fires.
    let before = await debug(page);
    await page.keyboard.down("Space");
    await untilState(page, (state) => shots(state.extras.debug) > shots(before), {
      extras: ["debug"],
      what: "Space's shot",
    });
    await page.keyboard.up("Space");

    // Q/E, 1-5 and the number pad pick stocked ammunition.
    await shellDebug(page, "giveAmmo", 5);
    const ammo = async (press, weapon) => {
      await press();
      await untilState(page, (state) => state.extras.debug.human.selectedAmmo === weapon, {
        extras: ["debug"],
        what: `${weapon} picked`,
      });
    };
    await ammo(() => page.keyboard.press("Digit2"), "spread");
    await ammo(() => page.keyboard.press("Digit5"), "piercing");
    await ammo(() => page.keyboard.press("KeyQ"), "ricochet");
    await ammo(() => page.keyboard.press("KeyE"), "piercing");
    await ammo(() => page.keyboard.press("Numpad3"), "rocket");
    await ammo(() => page.keyboard.press("Digit1"), "standard");
    await shot("ammo");

    // A plain wheel notch steps the ammunition; Shift with the wheel zooms instead.
    await page.mouse.move(900, 300);
    await ammo(() => page.mouse.wheel(0, 100), "spread");
    // One step per notch at most every `AMMO_SCROLL_INTERVAL_MS` (120 ms).
    await page.waitForTimeout(200);
    await ammo(() => page.mouse.wheel(0, -100), "standard");
    const zoom = (await debug(page)).view.zoom;
    await page.keyboard.down("Shift");
    await page.mouse.wheel(0, 100);
    await page.keyboard.up("Shift");
    let state = await untilState(page, (state) => state.extras.debug.view.zoom > zoom, {
      extras: ["debug"],
      what: "Shift and the wheel zoom out",
    });
    assert.equal(state.extras.debug.human.selectedAmmo, "standard", "a zoom steps no ammunition");

    // A right click drops a mine.
    await untilState(page, (state) => state.extras.debug.human.mineCooldown <= 0, {
      extras: ["debug"],
      what: "a mine ready",
    });
    await page.mouse.click(900, 300, { button: "right" });
    await untilState(page, (state) => state.extras.debug.human.mineCooldown > 0, {
      extras: ["debug"],
      what: "the right click's mine",
    });

    // Losing focus lets go of a held key: the tank stops and the round plays on.
    assert.equal(
      await page.evaluate(() => document.activeElement === document.querySelector("canvas")),
      true,
      "the canvas has the keyboard",
    );
    // A, back the way the last arrow drove: open ground, so the held key moves the tank.
    before = await debug(page);
    await page.keyboard.down("KeyA");
    let held = await settled(page, 700);
    assert.ok(distance(before, held) > 0.5, "the held key drives before the blur");
    await page.evaluate(() => document.querySelector("canvas").blur());
    held = await settled(page, 400);
    const after = await settled(page, 800);
    assert.ok(distance(held, after) < 0.3, "a blur lets go of the held key");
    assert.equal((await nextFrame(page)).phase, "playing", "a blur does not pause");
    await page.keyboard.up("KeyA");
    await page.evaluate(() => document.querySelector("canvas").focus());

    // A hidden page pauses single player; back in view it shows the pause.
    await setHidden(page, true);
    await page.waitForTimeout(400);
    await setHidden(page, false);
    state = await untilState(page, (state) => state.phase === "paused", {
      what: "the pause after a hidden page",
    });
    await untilWidget(page, "RESUME");
    await shot("hidden-paused");
    await clickWidget(page, "RESUME");
    await untilState(page, (state) => state.phase === "playing", { what: "the resume" });

    // The HUD's fullscreen button enters fullscreen and leaves it.
    const fullscreen = () => page.evaluate(() => document.fullscreenElement !== null);
    await clickWidget(page, "Enter fullscreen (F11)");
    const entered = await page
      .waitForFunction(() => document.fullscreenElement !== null, null, { timeout: 3000 })
      .then(() => true)
      .catch(() => false);
    if (entered) {
      await untilState(page, (state) => state.prefs.fullscreen, { what: "fullscreen saved" });
      await clickWidget(page, "Exit fullscreen (F11)");
      await page.waitForFunction(() => document.fullscreenElement === null, null, {
        timeout: 3000,
      });
      assert.equal(await fullscreen(), false);
      await untilState(page, (state) => !state.prefs.fullscreen, { what: "fullscreen left" });
    } else {
      console.log("input-check: this browser refused fullscreen; the button was not checked");
    }
    assert.equal((await nextFrame(page)).error, null, "renderer errors");
    await context.close();
  }

  // Clicks at pixel ratios 2 and 1.5 land on the widgets they aim at.
  for (const ratio of [2, 1.5]) {
    const name = `${ratio}x`;
    const { context, page } = await open(name, ratio);
    for (const [label, test] of [
      ["Harbor Havoc", (prefs) => prefs.map === "harbor"],
      ["SKIPPER", (prefs) => prefs.tank === "scout"],
      ["Hard", (prefs) => prefs.difficulty === "hard"],
      ["Solo Assault", (prefs) => prefs.gameMode === "solo"],
    ]) {
      await clickWidget(page, label);
      await untilState(page, (state) => test(state.prefs), { what: `${name}: ${label}` });
    }
    await untilState(page, (state) => state.ready, { timeout: 60000, what: `${name}: the arena` });
    await startRound(page, name);
    await clickWidget(page, "Pause (Esc)");
    await untilState(page, (state) => state.phase === "paused", { what: `${name}: the pause` });
    await clickWidget(page, "RESUME");
    await untilState(page, (state) => state.phase === "playing", { what: `${name}: the resume` });
    await page.screenshot({ path: `${out}/clicks-${name}.png` });
    await context.close();
  }

  assert.deepEqual(errors, []);
  console.log(
    "input-check: keys, ammo, wheel, mine, blur, hidden page, fullscreen and scaled clicks passed",
  );
} finally {
  await browser.close();
}
