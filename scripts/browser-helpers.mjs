import { chromium } from "playwright";

/**
 * Browser checks run in headless Chrome so they never pop a window over the desktop;
 * WebGPU and Playwright's coordinate mouse/touch input work the same there. Set
 * SLOPPY_HEADED=1 to watch a check in a visible window.
 */
export const headless = !process.env.SLOPPY_HEADED;

/** The Vite dev server under test; `pnpm run dev` prints it. */
export const gameUrl = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";

/** SLOPPY_WEBGL=1 opens the game on its WebGL2 build (`?webgl`, `openShell`). */
const webglOnly = Boolean(process.env.SLOPPY_WEBGL);

/** Headless Chrome whose pages keep their timers and frames while another page is in front. */
export function launchChrome() {
  return chromium.launch({
    channel: "chrome",
    headless,
    args: [
      "--disable-background-timer-throttling",
      "--disable-renderer-backgrounding",
      "--disable-backgrounding-occluded-windows",
    ],
  });
}

/** Chrome's touch emulation warns of its own touch events, which cannot be cancelled. */
const TOUCH_EMULATION = /Ignored attempt to cancel a touch\w+ event with cancelable=false/;

/**
 * Collect a page's uncaught errors in `errors`, each after `prefix`. `consoleErrors` also
 * collects console.error output (shader compilation failures are only reported there).
 * @param {import("playwright").Page} page
 * @param {string[]} errors
 * @param {{ consoleErrors?: boolean, prefix?: string }} [options]
 */
export function collectErrors(page, errors, { consoleErrors = false, prefix = "" } = {}) {
  page.on("pageerror", (error) => errors.push(prefix + error.message));
  if (!consoleErrors) return;
  page.on("console", (message) => {
    // Fixture pages have no icon; Chrome's automatic favicon request is not an error.
    if (message.type() !== "error" || message.location().url.endsWith("/favicon.ico")) return;
    if (TOUCH_EMULATION.test(message.text())) return;
    errors.push(prefix + message.text());
  });
}

/**
 * Launch installed Chrome (`launchChrome`) with one context and page, a desktop's unless
 * the device options say otherwise (`hasTouch` and `isMobile` make a touch screen with a
 * coarse pointer). Errors from every page in the context are collected in `errors`
 * (`collectErrors`).
 * @param {{ viewport?: { width: number, height: number }, consoleErrors?: boolean,
 *   deviceScaleFactor?: number, hasTouch?: boolean, isMobile?: boolean }} [options]
 */
export async function launchGame({
  viewport = { width: 1600, height: 900 },
  consoleErrors = false,
  deviceScaleFactor = 1,
  hasTouch = false,
  isMobile = false,
} = {}) {
  const browser = await launchChrome();
  const context = await browser.newContext({
    viewport,
    screen: viewport,
    deviceScaleFactor,
    hasTouch,
    isMobile,
  });
  const errors = [];
  context.on("page", (page) => collectErrors(page, errors, { consoleErrors }));
  const page = await context.newPage();
  return { browser, context, page, errors };
}

/**
 * Open the game page with its automation hook (`?automation`, the shell's
 * `crates/desktop/src/automation.rs`), on WebGL2 with SLOPPY_WEBGL=1, and wait for the
 * shell's first frame. `query` adds
 * the page's own parameters (`"debug&map=harbor"`). Throws if the shell failed to start.
 * @param {import("playwright").Page} page
 * @param {string} [query]
 */
export async function openShell(page, query = "") {
  const url = new URL(gameUrl);
  const parameters = ["automation", ...(webglOnly ? ["webgl"] : []), ...(query ? [query] : [])];
  url.search = parameters.join("&");
  await page.goto(url.href);
  await page.waitForFunction(() => document.documentElement.dataset.shell, null, {
    timeout: 60000,
  });
  const shell = await page.evaluate(() => document.documentElement.dataset.shell);
  if (shell !== "ready") throw new Error(`the shell did not start: ${shell}`);
  return nextFrame(page);
}

/**
 * The shell's state after its next presented frame, with the engine's JSON for each of
 * `extras` (`"hud"`, `"stats"`, `"nerd"`, `"debug"`, `"snapshot"`) in `state.extras`.
 * @param {import("playwright").Page} page
 * @param {string[]} [extras]
 */
export function nextFrame(page, extras = []) {
  return page.evaluate((wanted) => window.sloppyShell.frame(wanted), extras);
}

/**
 * Wait until `test(state)` holds for a presented frame; returns that state. `extras` as
 * in `nextFrame`.
 * @param {import("playwright").Page} page
 * @param {(state: any) => unknown} test
 * @param {{ extras?: string[], timeout?: number, what?: string }} [options]
 */
export async function untilState(page, test, { extras = [], timeout = 30000, what } = {}) {
  const start = Date.now();
  let state;
  while (Date.now() - start < timeout) {
    state = await nextFrame(page, extras);
    if (test(state)) return state;
  }
  const last = JSON.stringify({ ...state, prefs: undefined, extras: undefined });
  throw new Error(`timed out waiting for ${what ?? test.toString()}: ${last}`);
}

/**
 * The widget labelled `label` on the last frame (a string matches the whole label, a
 * RegExp any part), or undefined.
 * @param {import("playwright").Page} page
 * @param {string | RegExp} label
 */
export async function findWidget(page, label) {
  const widgets = await page.evaluate(() => window.sloppyShell.widgets());
  return widgets.find((widget) =>
    typeof label === "string" ? widget.label === label : label.test(widget.label),
  );
}

/**
 * The widget labelled `label` once a frame lists it (an egui area that has just opened
 * lists its widgets a frame later).
 * @param {import("playwright").Page} page
 * @param {string | RegExp} label
 */
export async function untilWidget(page, label, { timeout = 15000 } = {}) {
  const start = Date.now();
  for (;;) {
    const widget = await findWidget(page, label);
    if (widget) return widget;
    if (Date.now() - start > timeout) throw new Error(`no widget ${label} on screen`);
    await nextFrame(page);
  }
}

/**
 * Click the middle of the widget labelled `label` with the real mouse, at its CSS
 * coordinates, once a frame shows it, and wait for the frame that takes the click.
 * Returns the widget.
 * @param {import("playwright").Page} page
 * @param {string | RegExp} label
 * @param {{ timeout?: number }} [options]
 */
export async function clickWidget(page, label, { timeout = 15000 } = {}) {
  const start = Date.now();
  for (;;) {
    const widget = await findWidget(page, label);
    if (widget?.enabled) {
      await page.mouse.click(widget.x + widget.width / 2, widget.y + widget.height / 2);
      // egui reads one click per widget a frame: let it see this one before the next.
      await nextFrame(page);
      return widget;
    }
    if (Date.now() - start > timeout) {
      const labels = (await page.evaluate(() => window.sloppyShell.widgets())).map(
        (shown) => shown.label,
      );
      throw new Error(`no enabled widget ${label} on screen; shown: ${labels.join(", ")}`);
    }
    await nextFrame(page);
  }
}

/**
 * Run a debug command of the hook (the old `window.sloppy` methods: `killHuman`,
 * `giveAmmo`, `overview`, `autoplay`, `zoom`, `soak`, a room's `latency`).
 * @param {import("playwright").Page} page
 * @param {string} name
 * @param {...unknown} args
 */
export function shellDebug(page, name, ...args) {
  return page.evaluate(
    ([command, rest]) => window.sloppyShell.debug(command, ...rest),
    [name, args],
  );
}

/**
 * Fingers on a touch page (`hasTouch`) through CDP, as a touch screen sends them: a start
 * or move carries every finger down, an end the one that lifts. Once one has lifted the
 * others hold still and lift last. Each waits for the shell's next frame.
 * @param {import("playwright").Page} page
 */
export async function fingers(page) {
  const session = await page.context().newCDPSession(page);
  const down = new Map();
  // CDP ends its touch sequence at a touchEnd although fingers stay down: they can still
  // lift together (an empty touchEnd) but no longer move or lift one by one.
  let lifted = false;
  const send = async (type, touchPoints) => {
    await session.send("Input.dispatchTouchEvent", { type, touchPoints });
    await nextFrame(page);
  };
  const point = (id, x, y) => ({ id, x, y, radiusX: 4, radiusY: 4, force: 1 });
  return {
    start(id, x, y) {
      down.set(id, point(id, x, y));
      return send("touchStart", [...down.values()]);
    },
    move(id, x, y) {
      if (lifted) throw new Error("CDP moves no finger after another lifted");
      down.set(id, point(id, x, y));
      return send("touchMove", [...down.values()]);
    },
    end(id) {
      const finger = down.get(id);
      down.delete(id);
      if (!lifted) {
        lifted = down.size > 0;
        return send("touchEnd", [finger]);
      }
      if (down.size > 0) throw new Error("CDP lifts the rest only together");
      lifted = false;
      return send("touchEnd", []);
    },
    async tap(id, x, y) {
      await this.start(id, x, y);
      await this.end(id);
    },
  };
}

/**
 * Tap the middle of the widget labelled `label` with a finger (`fingers`) once a frame
 * shows it enabled. Returns the widget.
 * @param {import("playwright").Page} page
 * @param {Awaited<ReturnType<typeof fingers>>} touch
 * @param {string | RegExp} label
 */
export async function tapWidget(page, touch, label, { timeout = 15000 } = {}) {
  const start = Date.now();
  for (;;) {
    const widget = await findWidget(page, label);
    if (widget?.enabled) {
      await touch.tap(90, widget.x + widget.width / 2, widget.y + widget.height / 2);
      return widget;
    }
    if (Date.now() - start > timeout) throw new Error(`no enabled widget ${label} to tap`);
    await nextFrame(page);
  }
}
