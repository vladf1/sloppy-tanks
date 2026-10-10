import assert from "node:assert/strict";
import { chromium } from "playwright";

/**
 * Browser checks run in headless Chrome so they never pop a window over the desktop;
 * WebGPU and Playwright's coordinate mouse/touch input work the same there. Set
 * SLOPPY_HEADED=1 to watch a check in a visible window.
 */
export const headless = !process.env.SLOPPY_HEADED;

/** The Vite dev server under test; `pnpm run dev` prints it. */
export const gameUrl = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";

/**
 * SLOPPY_WEBGL=1 runs checks on the WebGL2 engine: their pages see no WebGPU, so they
 * fall back to WebGL by themselves, as a browser without WebGPU does.
 */
const webglOnly = Boolean(process.env.SLOPPY_WEBGL);

/** Headless Chrome whose pages keep their timers and frames while another page is in front,
 * so several players on one machine all keep sending input. */
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
 * Launch installed Chrome (`launchChrome`) with one desktop context and page, with
 * `window.engine` (`installEngineHelpers`) in every page. Errors from every page in the
 * context are collected in `errors` (`collectErrors`).
 * `isMobile` emulates a phone: its screen is the viewport and its pointer is coarse.
 * @param {{ viewport?: { width: number, height: number }, hasTouch?: boolean, isMobile?: boolean, consoleErrors?: boolean }} [options]
 */
export async function launchGame({
  viewport = { width: 1600, height: 900 },
  hasTouch = false,
  isMobile = false,
  consoleErrors = false,
} = {}) {
  const browser = await launchChrome();
  const context = await browser.newContext({
    viewport,
    screen: viewport,
    hasTouch,
    isMobile,
    deviceScaleFactor: 1,
  });
  await installEngineHelpers(context);
  const errors = [];
  context.on("page", (page) => collectErrors(page, errors, { consoleErrors }));
  const page = await context.newPage();
  return { browser, context, page, errors };
}

/**
 * Hold the game's own `loop` animation callback so a check decides exactly when
 * frames run: `window.advanceFrame(ms)` runs one loop frame `ms` after the previous
 * one, and `window.runLoop(timestamp)` runs one at an absolute RAF timestamp. The
 * loop is the one callback that hands the packed input to `Game.frame`, so every
 * engine frame (simulation steps, events, HUD refresh and drawing) happens only
 * then. Other animation callbacks keep running normally.
 * @param {import("playwright").Page} page
 */
export async function freezeLoop(page) {
  await page.addInitScript(() => {
    let loop, now;
    const requestFrame = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (callback) => {
      if (callback.name !== "loop") return requestFrame(callback);
      loop = callback;
      return 1;
    };
    window.runLoop = (timestamp) => {
      now = timestamp;
      loop(timestamp);
    };
    window.advanceFrame = (ms) => window.runLoop((now ?? performance.now()) + ms);
  });
}

/**
 * Install `window.engine` in every page of `target`: shorthands for the dev-only
 * `Game.debug_*` fixture hooks (`crates/web/src/game/debug.rs`) that parse their
 * JSON. `engine.view()` is what every entity's view showed in the last frame and
 * `engine.covers()` the covers; the simulation and camera are `window.sloppy.sim` and the
 * renderer counters `window.sloppy.stats()`. Use it once `window.sloppy` exists. With
 * `webglOnly` its pages also see no WebGPU.
 * @param {import("playwright").BrowserContext | import("playwright").Page} target
 */
export async function installEngineHelpers(target) {
  if (webglOnly) {
    await target.addInitScript(() => {
      Object.defineProperty(Navigator.prototype, "gpu", { get: () => undefined });
    });
  }
  await target.addInitScript(() => {
    const game = () => window.sloppy.game;
    window.engine = {
      view: () => JSON.parse(game().debug_view_json()),
      covers: () => JSON.parse(game().debug_covers_json()),
      /** Draw one still frame, from `camera = [px, py, pz, tx, ty, tz]` when given. */
      draw: (camera = [], overview = false) =>
        game().debug_render(1, 0, overview, new Float32Array(camera)),
      setTank: (id, patch) => game().debug_set_tank(id, JSON.stringify(patch)),
      setHuman: (patch) => game().debug_set_tank(window.sloppy.sim.human.id, JSON.stringify(patch)),
      setSim: (patch) => game().debug_set_sim(JSON.stringify(patch)),
    };
  });
}

/**
 * Start a round through the real Battle Setup menu, as a player does. Until GO
 * completes, the startup overlay covers the canvas and swallows pointer and wheel
 * input; `window.sloppy.start()` alone restarts the simulation but leaves the menu up.
 * Waits on match state rather than rendered time, so it also works in fixtures that
 * freeze the game's own animation loop. `touch` taps GO like a tablet player.
 * @param {import("playwright").Page} page
 * @param {{ touch?: boolean }} [options]
 */
export async function startRound(page, { touch = false } = {}) {
  const start = page.locator("#start");
  await (touch ? start.tap() : start.click());
  // Production builds have no `window.sloppy`; there the removed menu is the signal.
  await page.waitForFunction(
    () =>
      !document.querySelector("#startup-overlay") &&
      (!window.sloppy || window.sloppy.sim.match.phase === "playing"),
  );
}

/**
 * Wait until Battle Setup is ready to start a round, polling every animation frame.
 * @param {import("playwright").Page} page
 * @param {{ timeout?: number }} [options]
 */
export function menuReady(page, options) {
  return page.waitForFunction(
    () => document.querySelector("#startup-overlay")?.dataset.state === "ready",
    null,
    options,
  );
}

/**
 * Choose a map in Battle Setup with pointer clicks, as a player does: a button in the row
 * of standard maps, or the dropdown that replaces it with `?debug`. Single player
 * and a new room share the "mapMode" choice.
 * @param {import("playwright").Page} page
 * @param {string} map
 */
export async function chooseMap(page, map) {
  const dropdown = page.locator('.map-picker[data-name="mapMode"] .map-picker-button');
  if (await dropdown.isVisible()) {
    await dropdown.click();
    await page.locator(`#mapMode-${map}`).click();
  } else {
    await page.locator(`input[name="mapMode"][value="${map}"]`).click();
  }
  await page.waitForFunction(
    (map) => document.querySelector('.map-picker[data-name="mapMode"]')?.dataset.value === map,
    map,
  );
}

/** The map Battle Setup shows as chosen. @param {import("playwright").Page} page */
export function chosenMap(page) {
  return page.locator('.map-picker[data-name="mapMode"]').getAttribute("data-value");
}

/** Set only the seed the startup script hands the engine; the page's other
 * `Math.random` uses (cosmetic variation, bot names) stay untouched. */
export async function seedGame(page, seed) {
  if (!Number.isSafeInteger(seed) || seed < 0) throw new Error("Invalid fixture seed");
  // Startup is bundled inline in HTML in both Vite modes.
  await page.route(
    (url) => url.pathname.endsWith("/") || url.pathname.endsWith(".html"),
    async (route) => {
      const url = new URL(route.request().url());
      if (["127.0.0.1", "localhost"].includes(url.hostname)) {
        // Chromium treats a fulfilled document as non-local; allow this fixture's Vite socket.
        await page.context().grantPermissions(["local-network-access"], { origin: url.origin });
      }
      const response = await route.fetch();
      const source = await response.text();
      const seeded = source.replace(
        /Math\.floor\(Math\.random\(\)\s*\*\s*(?:1e6|1000000)\)/,
        String(seed),
      );
      if (source === seeded) throw new Error("Game seed initialization changed; update seedGame");
      await route.fulfill({ response, body: seeded });
    },
  );
}

/**
 * A physical coordinate click at the centre of `target` (a selector or a locator), which
 * exercises the same pointer routing a player uses; locator clicks can bypass
 * pointer-event and hit-testing bugs.
 * @param {import("playwright").Page} page
 * @param {string | import("playwright").Locator} target
 */
export async function click(page, target) {
  const locator = typeof target === "string" ? page.locator(target) : target;
  await locator.waitFor();
  await locator.scrollIntoViewIfNeeded();
  assert.ok(await locator.isEnabled(), `${target} is enabled`);
  const bounds = await locator.boundingBox();
  assert.ok(bounds, `${target}`);
  await page.mouse.click(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
}

/** Wait two animation frames, so input and layout changes reach the page and its canvas. */
export function nextFrames(page) {
  return page.evaluate(
    () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
  );
}

/** The bounding box of `selector`, which must be laid out. */
export async function box(page, selector) {
  const rect = await page.locator(selector).boundingBox();
  assert.ok(rect, selector);
  return rect;
}

/** The centre of `selector`'s bounding box. */
export async function center(page, selector) {
  const rect = await box(page, selector);
  return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
}

/**
 * Real multi-touch through CDP. `touch(type, id, x, y)` presses, moves or lifts
 * (`touchEnd`) finger `id` while the others stay down, then waits two frames;
 * `tap(id, point)` presses and lifts one finger; `cancel()` is the OS cancelling every touch.
 * @param {import("playwright").Page} page
 */
export async function touchScreen(page) {
  const session = await page.context().newCDPSession(page);
  const fingers = new Map();
  const touch = async (type, id, x, y) => {
    const released = fingers.get(id);
    if (type === "touchEnd") fingers.delete(id);
    else fingers.set(id, { id, x, y, radiusX: 5, radiusY: 5, force: 1 });
    await session.send("Input.dispatchTouchEvent", {
      type,
      touchPoints: type === "touchEnd" ? [released] : [...fingers.values()],
    });
    await nextFrames(page);
  };
  const tap = async (id, { x, y }) => {
    await touch("touchStart", id, x, y);
    await touch("touchEnd", id);
  };
  const cancel = async () => {
    await session.send("Input.dispatchTouchEvent", { type: "touchCancel", touchPoints: [] });
    fingers.clear();
  };
  return { touch, tap, cancel };
}

/** Each control's box lies inside the `width`×`height` viewport, and no two overlap. */
export function assertApart(rects, selectors, width, height) {
  const size = `at ${width}x${height}`;
  rects.forEach((rect, i) => {
    assert.ok(rect.x >= 0 && rect.y >= 0, `${selectors[i]} on screen ${size}`);
    assert.ok(
      rect.x + rect.width <= width && rect.y + rect.height <= height,
      `${selectors[i]} on screen ${size}`,
    );
    rects.slice(i + 1).forEach((other, j) => {
      const apart =
        rect.x + rect.width <= other.x ||
        other.x + other.width <= rect.x ||
        rect.y + rect.height <= other.y ||
        other.y + other.height <= rect.y;
      assert.ok(apart, `${selectors[i]} and ${selectors[i + j + 1]} do not overlap ${size}`);
    });
  });
}

/** A physical touch at the centre of each control lands on that control. */
export async function assertTouchable(page, selectors, size) {
  for (const selector of selectors) {
    const point = await center(page, selector);
    assert.equal(
      await page.evaluate(
        ({ selector, point }) =>
          document.querySelector(selector).contains(document.elementFromPoint(point.x, point.y)),
        { selector, point },
      ),
      true,
      `${selector} can receive physical touches ${size}`,
    );
  }
}

/** Single player's touch controls hold fire; the message shows the tank and match state. */
export async function assertFiring(page, message) {
  const now = await page.evaluate(() => ({
    fire: window.sloppy.controls.touch.fire,
    alive: window.sloppy.sim.human.alive,
    phase: window.sloppy.sim.match.phase,
  }));
  assert.equal(now.fire, true, `${message}: ${JSON.stringify(now)}`);
}

/** A finger on the arena at (x, y) fires until it lifts; returns where the turret then aims.
 * `touch` is a `touchScreen` finger driver. */
export async function aimAfterTap(page, touch, x, y) {
  await touch("touchStart", 2, x, y);
  await touch("touchMove", 2, x + 1, y);
  await assertFiring(page, "an arena finger fires");
  await page.waitForTimeout(400);
  await touch("touchEnd", 2);
  assert.equal(
    await page.evaluate(() => window.sloppy.controls.touch.fire),
    false,
    "lifting it stops firing",
  );
  return page.evaluate(() => window.sloppy.sim.human.aim);
}

/** Turn touch controls on or off and save Settings; `touch` taps like a tablet player. */
export async function setTouchMode(page, mode, { touch = false } = {}) {
  const press = (selector) =>
    touch ? page.locator(selector).tap() : page.locator(selector).click();
  await press("#settings-open");
  await page.locator("#touch-mode").selectOption(mode);
  await press(".settings-save");
}

/** A screenshot's RGBA pixels. Only the checks that compare pixels load the native canvas. */
export async function pixels(png) {
  const { createCanvas, loadImage } = await import("@napi-rs/canvas");
  const image = await loadImage(png);
  const canvas = createCanvas(image.width, image.height);
  const context = canvas.getContext("2d");
  context.drawImage(image, 0, 0);
  return context.getImageData(0, 0, image.width, image.height).data;
}
