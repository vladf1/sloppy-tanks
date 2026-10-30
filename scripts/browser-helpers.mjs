import { chromium } from "playwright";

/**
 * Browser checks run in headless Chrome so they never pop a window over the desktop;
 * WebGPU and Playwright's coordinate mouse/touch input work the same there. Set
 * SLOPPY_HEADED=1 to watch a check in a visible window.
 */
export const headless = !process.env.SLOPPY_HEADED;

/** The Vite dev server under test; `pnpm run dev` prints it. */
export const gameUrl = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";

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
    errors.push(prefix + message.text());
  });
}

/**
 * Launch installed Chrome (`launchChrome`) with one desktop context and page. Errors from
 * every page in the context are collected in `errors` (`collectErrors`).
 * @param {{ viewport?: { width: number, height: number }, consoleErrors?: boolean }} [options]
 */
export async function launchGame({
  viewport = { width: 1600, height: 900 },
  consoleErrors = false,
} = {}) {
  const browser = await launchChrome();
  const context = await browser.newContext({ viewport, screen: viewport, deviceScaleFactor: 1 });
  const errors = [];
  context.on("page", (page) => collectErrors(page, errors, { consoleErrors }));
  const page = await context.newPage();
  return { browser, context, page, errors };
}

/**
 * Open the game page with its automation hook (`?automation`, the shell's
 * `crates/desktop/src/automation.rs`) and wait for the shell's first frame. `query` adds
 * the page's own parameters (`"debug&map=harbor"`). Throws if the shell failed to start.
 * @param {import("playwright").Page} page
 * @param {string} [query]
 */
export async function openShell(page, query = "") {
  const url = new URL(gameUrl);
  url.search = query ? `automation&${query}` : "automation";
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
 * Click the middle of the widget labelled `label` with the real mouse, at its CSS
 * coordinates, once a frame shows it. Returns the widget.
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
