// The page's text fields type like the browser's own: Battle Setup's room code and name
// fields take typing, the clipboard's shortcuts (paste, copy and cut, on the browser's
// clipboard), Enter, Escape, Tab, the arrows, an IME's composition and a soft keyboard's
// text; on a touch screen the field's element has the page's focus by the time the
// tapping finger's release is handled, the only moment iOS brings up its keyboard; and
// once no field types, the keys drive the tank and the fingers the touch controls
// again. The shell draws on one canvas: its text entry is a hidden `<input>`
// (crates/desktop/src/text_entry.rs) that holds the page's focus while a field types.
// Chrome on a desktop (and its WebGL build), a phone and a tablet; WebKit on a desktop
// and a tablet.
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { webkit } from "playwright";
import {
  collectErrors,
  findWidget,
  headless,
  launchChrome,
  nextFrame,
  openShell,
  untilState,
} from "./browser-helpers.mjs";

const out = "artifacts/performance/text-fields-check";
mkdirSync(out, { recursive: true });

const ENTRY = "sloppy-text-entry";
const CODE = "Room code";
const NAME = "Tank driver";
/** The command key people press for the clipboard here, which Playwright's browsers
 * take as theirs: Cmd on a Mac, Ctrl elsewhere. */
const COMMAND = process.platform === "darwin" ? "Meta" : "Control";
/** How long a wait for the shell to show something lasts. */
const TIMEOUT = 8000;

/** The element's attributes, which keep a soft keyboard from changing what is typed. */
const ATTRIBUTES = {
  type: "text",
  inputmode: "text",
  enterkeyhint: "done",
  autocapitalize: "off",
  autocomplete: "off",
  autocorrect: "off",
  spellcheck: "false",
};

/** Before the page runs: note which element has the focus once each release of a
 * mouse button or finger has been handled (the canvas's own listeners run first). */
function watchReleases() {
  window.textChecks = { atRelease: [] };
  addEventListener("pointerup", () => {
    const element = document.activeElement;
    window.textChecks.atRelease.push(element?.id || element?.tagName.toLowerCase() || "");
  });
}

/** The focused element: the text entry's id, or a tag such as `canvas`. */
function active(page) {
  return page.evaluate(() => {
    const element = document.activeElement;
    return element?.id || element?.tagName.toLowerCase() || "";
  });
}

async function until(page, test, what) {
  const start = Date.now();
  let last;
  while (Date.now() - start < TIMEOUT) {
    last = await test();
    if (last === true) return;
    await nextFrame(page);
  }
  throw new Error(`timed out waiting for ${what} (last: ${JSON.stringify(last)})`);
}

/** Wait for the focus to be on `wanted`. */
function untilActive(page, wanted, what) {
  return until(
    page,
    async () => {
      const now = await active(page);
      return now === wanted || now;
    },
    `${what}: the focus on ${wanted}`,
  );
}

async function widget(page, label) {
  let found;
  await until(
    page,
    async () => Boolean((found = await findWidget(page, label))),
    `the widget ${label}`,
  );
  return found;
}

const middle = (shown) => ({ x: shown.x + shown.width / 2, y: shown.y + shown.height / 2 });

/** Save the page as `name`, a few frames on: WebKit's capture shows an older frame. */
async function screenshot(page, name) {
  for (let frame = 0; frame < 3; frame += 1) await nextFrame(page);
  return page.screenshot({ path: `${out}/${name}.png` });
}

/** Wait for the field labelled `label` to hold `value`. */
function untilValue(page, label, value, what) {
  return until(
    page,
    async () => {
      const now = (await findWidget(page, label))?.value;
      return now === value || now;
    },
    `${what}: ${label} holding ${JSON.stringify(value)}`,
  );
}

/** What the browser's clipboard holds, where the page may read it (Chrome, granted). */
function clipboard(page) {
  return page.evaluate(() => navigator.clipboard.readText());
}

function setClipboard(page, text) {
  return page.evaluate((text) => navigator.clipboard.writeText(text), text);
}

/** The last release's focus: the text entry's when the release focused it. */
async function focusAtRelease(page) {
  return page.evaluate(() => window.textChecks.atRelease.at(-1));
}

/** Press and release a mouse button on `point`, a frame apart (`slow`) or at once. */
async function click(page, point, slow) {
  if (!slow) return page.mouse.click(point.x, point.y);
  await page.mouse.move(point.x, point.y);
  await nextFrame(page);
  await page.mouse.down();
  await nextFrame(page);
  await page.mouse.up();
}

/** A finger's tap on `point`: held over a frame through Chrome's DevTools protocol, or
 * Playwright's instant tap. */
async function tap(page, cdp, point) {
  if (!cdp) return page.touchscreen.tap(point.x, point.y);
  await cdp.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ x: point.x, y: point.y, id: 1 }],
  });
  await nextFrame(page);
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
}

/** Push a finger from `from` by `by` and hold it there for `ms`. */
async function drag(page, cdp, from, by, ms) {
  const point = (share) => ({ x: from.x + by.x * share, y: from.y + by.y * share, id: 1 });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [point(0)] });
  for (let step = 1; step <= 6; step += 1) {
    await nextFrame(page);
    await cdp.send("Input.dispatchTouchEvent", {
      type: "touchMove",
      touchPoints: [point(step / 6)],
    });
  }
  await page.waitForTimeout(ms);
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
}

/** Focus the field labelled `label` with `press`; the text entry has the focus once
 * the press lets go, and sits over the field. */
async function focusField(page, label, press, what) {
  const field = await widget(page, label);
  await press(middle(field));
  assert.equal(await focusAtRelease(page), ENTRY, `${what}: ${label} focused at the release`);
  await nextFrame(page);
  await nextFrame(page);
  assert.equal(await active(page), ENTRY, `${what}: ${label} still has the text entry`);
  const placed = await page.evaluate((id) => {
    const element = document.getElementById(id);
    const rect = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    return {
      x: rect.left,
      y: rect.top,
      opacity: style.opacity,
      display: style.display,
      fontSize: style.fontSize,
      pointerEvents: style.pointerEvents,
      attributes: Object.fromEntries(
        ["type", "inputmode", "enterkeyhint", "autocapitalize"]
          .concat(["autocomplete", "autocorrect", "spellcheck"])
          .map((name) => [name, element.getAttribute(name)]),
      ),
    };
  }, ENTRY);
  assert.deepEqual(placed.attributes, ATTRIBUTES, `${what}: the text entry's attributes`);
  assert.deepEqual(
    [placed.opacity, placed.display, placed.fontSize, placed.pointerEvents],
    ["0", "block", "16px", "none"],
    `${what}: the text entry is invisible, focusable and out of the fingers' way`,
  );
  // egui's caret starts a little left of the text.
  const slack = 4;
  const inside =
    placed.x >= field.x - slack &&
    placed.x <= field.x + field.width + slack &&
    placed.y >= field.y - slack &&
    placed.y <= field.y + field.height + slack;
  assert.ok(inside, `${what}: the text entry at ${placed.x},${placed.y} sits in ${label}`);
}

/** Select all of the focused field and wait a frame for egui to settle the selection. */
async function selectAll(page) {
  await page.keyboard.press(`${COMMAND}+KeyA`);
  await nextFrame(page);
  await nextFrame(page);
}

/** Typing, the clipboard, Enter, Escape and the arrows with a keyboard. `readable`: the
 * check may read the clipboard (Chrome with the permission). */
async function keyboardFields(page, what, { readable, cdp }) {
  const shot = (name) => screenshot(page, `${what}-${name}`);
  await focusField(page, CODE, (point) => click(page, point, true), what);
  await page.keyboard.type("abcd2345");
  await untilValue(page, CODE, "abcd2345", `${what}: typing`);
  await shot("typed");

  // Copy and cut put the selection on the browser's clipboard; paste brings it back.
  if (readable) await setClipboard(page, "");
  await selectAll(page);
  await page.keyboard.press(`${COMMAND}+KeyC`);
  if (readable) assert.equal(await clipboard(page), "abcd2345", `${what}: copy`);
  await page.keyboard.press(`${COMMAND}+KeyX`);
  await untilValue(page, CODE, "", `${what}: cut`);
  if (readable) assert.equal(await clipboard(page), "abcd2345", `${what}: cut's clipboard`);
  await page.keyboard.press(`${COMMAND}+KeyV`);
  await untilValue(page, CODE, "abcd2345", `${what}: pasting the cut`);
  await setClipboard(page, "wxyz2345");
  await selectAll(page);
  await page.keyboard.press(`${COMMAND}+KeyV`);
  await untilValue(page, CODE, "wxyz2345", `${what}: paste`);
  // Ctrl+V on a Mac pastes nowhere, as in the browser's own fields: no stray "v".
  await page.keyboard.press("Control+KeyV");
  await nextFrame(page);
  await nextFrame(page);
  const pasted = (await findWidget(page, CODE)).value;
  assert.ok(!pasted.includes("v"), `${what}: no v typed by a paste shortcut: ${pasted}`);
  await shot("pasted");

  // Escape lets go of the field; the canvas has the keys again.
  await page.keyboard.press("Escape");
  await untilActive(page, "canvas", `${what}: Escape`);

  // Enter finds the typed code, which reads in capitals, and lets go of the field.
  await focusField(page, CODE, (point) => click(page, point, false), `${what} again`);
  await selectAll(page);
  await page.keyboard.type("abcd2345");
  await page.keyboard.press("Enter");
  await untilValue(page, CODE, "ABCD2345", `${what}: Enter`);
  await untilActive(page, "canvas", `${what}: Enter`);

  // The arrows, Backspace, Home and End and a selection with Shift edit at the caret.
  await focusField(page, NAME, (point) => click(page, point, false), `${what}: the name`);
  await selectAll(page);
  await page.keyboard.type("ab");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.type("X");
  await untilValue(page, NAME, "aXb", `${what}: ArrowLeft`);
  await page.keyboard.press("Backspace");
  await untilValue(page, NAME, "ab", `${what}: Backspace`);
  await page.keyboard.press("End");
  await page.keyboard.type(" Pilot");
  await untilValue(page, NAME, "ab Pilot", `${what}: End`);
  await page.keyboard.press("Shift+Home");
  await nextFrame(page);
  await page.keyboard.type("Typist");
  await untilValue(page, NAME, "Typist", `${what}: Shift+Home`);

  // A soft keyboard's or an IME's text comes with no key: an input event only, or a
  // composition (Chrome's DevTools protocol stages one).
  await page.keyboard.insertText("ü");
  await untilValue(page, NAME, "Typistü", `${what}: text with no key`);
  if (cdp) {
    await cdp.send("Input.imeSetComposition", {
      text: "せんしゃ",
      selectionStart: 4,
      selectionEnd: 4,
    });
    await untilValue(page, NAME, "Typistüせんしゃ", `${what}: a composition`);
    await cdp.send("Input.insertText", { text: "戦車" });
    await untilValue(page, NAME, "Typistü戦車", `${what}: its commit`);
  }
  await shot("name");

  // Tab moves egui's focus on and types no tab.
  await page.keyboard.press("Tab");
  await nextFrame(page);
  await nextFrame(page);
  assert.ok(!(await findWidget(page, NAME)).value.includes("\t"), `${what}: Tab typed`);
  await page.keyboard.press("Escape");
  await untilActive(page, "canvas", `${what}: Escape after Tab`);
}

/** Single player's round from Battle Setup's Multiplayer tab. */
async function startRound(page, press, what) {
  const tab = await widget(page, "SINGLE PLAYER");
  await press(middle(tab));
  await untilState(page, (state) => !state.multiplayerTab && state.ready, {
    what: `${what}: Battle Setup's arena`,
    timeout: 60000,
  });
  const go = await widget(page, "GO!");
  await press(middle(go));
  return untilState(page, (state) => state.inRound && state.phase === "playing", {
    extras: ["debug"],
    what: `${what}: the round`,
  });
}

const distance = (a, b) => Math.hypot(a.x - b.x, a.z - b.z);

/** After the fields, W drives the tank. */
async function drive(page, what) {
  const state = await startRound(page, (point) => click(page, point, false), what);
  assert.equal(await active(page), "canvas", `${what}: the canvas has the keys`);
  const start = state.extras.debug.human;
  await page.keyboard.down("KeyW");
  await page.waitForTimeout(700);
  await page.keyboard.up("KeyW");
  const after = (await nextFrame(page, ["debug"])).extras.debug.human;
  assert.ok(distance(start, after) > 1, `${what}: W drove ${distance(start, after)} m`);
}

/** A touch screen's fields: a tap focuses the text entry at its release, a soft
 * keyboard types, putting the keyboard away (iOS's Done) lets go of the field, and the
 * touch controls drive the round after. A phone's Multiplayer tab has no fields (it
 * joins the busiest room): there the text entry never takes a finger's focus. */
async function touchFields(page, what, cdp, phone) {
  const shot = (name) => screenshot(page, `${what}-${name}`);
  const press = (point) => tap(page, cdp, point);
  if (phone) {
    const widgets = await page.evaluate(() => window.sloppyShell.widgets());
    const fields = widgets.filter((shown) => shown.role === "TextInput");
    assert.deepEqual(fields, [], `${what}: a phone's Multiplayer tab has no fields`);
  } else {
    await touchTyping(page, what, cdp, press);
  }
  await shot("setup");

  const state = await startRound(page, press, what);
  assert.equal(await active(page), "canvas", `${what}: the canvas has the fingers`);
  assert.ok(state.touchControls, `${what}: the touch controls show`);
  if (cdp) {
    // The drive stick, at the bottom left (`TouchLayout`), pushed forward.
    const viewport = page.viewportSize();
    const stick = phone
      ? { x: 12 + 75, y: viewport.height - 12 - 75 }
      : { x: 24 + 88, y: viewport.height - 28 - 88 };
    await drag(page, cdp, stick, { x: 0, y: -60 }, 900);
    const after = (await nextFrame(page, ["debug"])).extras.debug.human;
    const moved = distance(state.extras.debug.human, after);
    assert.ok(moved > 1, `${what}: the stick drove ${moved} m`);
  }
  await shot("round");
  const releases = await page.evaluate(() => window.textChecks.atRelease);
  if (phone) {
    assert.ok(!releases.includes(ENTRY), `${what}: no finger focused the text entry`);
  }
  assert.equal(await active(page), "canvas", `${what}: the canvas kept the fingers`);
}

/** Typing on a touch screen's fields, with a soft keyboard's text. */
async function touchTyping(page, what, cdp, press) {
  const shot = (name) => screenshot(page, `${what}-${name}`);
  await focusField(page, NAME, press, what);
  await selectAll(page);
  await page.keyboard.insertText("Thumbs");
  await untilValue(page, NAME, "Thumbs", `${what}: a soft keyboard's text`);
  await page.keyboard.press("Backspace");
  await untilValue(page, NAME, "Thumb", `${what}: its Backspace`);
  // Playwright's instant tap, with no frame of the finger before it lifts.
  await focusField(page, CODE, (point) => page.touchscreen.tap(point.x, point.y), what);
  await page.keyboard.insertText("abcd2345");
  await untilValue(page, CODE, "abcd2345", `${what}: the room code`);
  await shot("typed");

  // The keyboard put away: the page's focus leaves the text entry for nothing.
  await page.evaluate(() => document.activeElement.blur());
  await untilActive(page, "canvas", `${what}: the keyboard put away`);
  await nextFrame(page);
  assert.notEqual(await active(page), ENTRY, `${what}: the field let go`);

  // The round's tab, tapped while a field types, lets go of it too (`startRound`).
  await focusField(page, NAME, press, `${what} again`);
}

const CASES = [
  {
    name: "chrome-desktop",
    engine: "chrome",
    context: { viewport: { width: 1280, height: 800 } },
  },
  {
    name: "chrome-webgl",
    engine: "chrome",
    query: "webgl",
    context: { viewport: { width: 1280, height: 800 } },
  },
  {
    name: "chrome-phone",
    engine: "chrome",
    context: { viewport: { width: 393, height: 852 }, hasTouch: true, isMobile: true },
  },
  {
    name: "chrome-tablet",
    engine: "chrome",
    context: { viewport: { width: 1180, height: 820 }, hasTouch: true, isMobile: true },
  },
  {
    name: "webkit-desktop",
    engine: "webkit",
    context: { viewport: { width: 1280, height: 800 } },
  },
  {
    name: "webkit-tablet",
    engine: "webkit",
    context: { viewport: { width: 1180, height: 820 }, hasTouch: true, isMobile: true },
  },
];

const wanted = process.argv.slice(2);
const browsers = {};
try {
  for (const { name, engine, query, context: options } of CASES) {
    if (wanted.length && !wanted.includes(name)) continue;
    browsers[engine] ??= await (engine === "chrome" ? launchChrome() : webkit.launch({ headless }));
    const context = await browsers[engine].newContext({ deviceScaleFactor: 1, ...options });
    if (engine === "chrome") {
      await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    }
    await context.addInitScript(watchReleases);
    const page = await context.newPage();
    const errors = [];
    collectErrors(page, errors, { consoleErrors: true });
    const cdp = engine === "chrome" ? await context.newCDPSession(page) : null;
    try {
      await openShell(page, query ? `multiplayer&${query}` : "multiplayer");
      await untilState(page, (state) => state.multiplayerTab, { what: `${name}: Multiplayer` });
      // The loading view fades out over the canvas, taking the presses meanwhile.
      await page.waitForFunction(
        () => getComputedStyle(document.getElementById("boot")).display === "none",
      );
      if (options.hasTouch) {
        await touchFields(page, name, cdp, options.viewport.width < 600);
      } else {
        await keyboardFields(page, name, { readable: engine === "chrome", cdp });
        await drive(page, name);
      }
    } catch (error) {
      await page.screenshot({ path: `${out}/${name}-failed.png` }).catch(() => {});
      throw error;
    }
    assert.equal((await nextFrame(page)).error, null, `${name}: renderer errors`);
    // The room directory need not run for the fields.
    const unexpected = errors.filter(
      (error) => !/Failed to load resource|ERR_CONNECTION_REFUSED|Could not connect/.test(error),
    );
    assert.deepEqual(unexpected, [], `${name}: page errors`);
    console.log(`text-fields-check: ${name} passed`);
    await context.close();
  }
  console.log("text-fields-check: typing, the clipboard and the fields' focus passed");
} finally {
  for (const browser of Object.values(browsers)) await browser.close();
}
