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
 * Launch installed Chrome with one desktop context and page, with `window.engine`
 * (`installEngineHelpers`) in every page. Page errors from every
 * page in the context are collected in `errors`; `consoleErrors` also collects
 * console.error output (shader compilation failures are only reported there).
 * @param {{ viewport?: { width: number, height: number }, hasTouch?: boolean, consoleErrors?: boolean }} [options]
 */
export async function launchGame({
  viewport = { width: 1600, height: 900 },
  hasTouch = false,
  consoleErrors = false,
} = {}) {
  const browser = await chromium.launch({
    channel: "chrome",
    headless,
    args: ["--disable-backgrounding-occluded-windows", "--disable-renderer-backgrounding"],
  });
  const context = await browser.newContext({ viewport, hasTouch, deviceScaleFactor: 1 });
  await installEngineHelpers(context);
  const errors = [];
  context.on("page", (page) => {
    page.on("pageerror", (error) => errors.push(error.message));
    if (consoleErrors) {
      page.on("console", (message) => {
        // Fixture pages have no icon; Chrome's automatic favicon request is not an error.
        if (message.type() !== "error" || message.location().url.endsWith("/favicon.ico")) return;
        errors.push(message.text());
      });
    }
  });
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
 * JSON. `engine.state()` is the simulation and camera (`debug_json`), `engine.view()`
 * what every entity's view showed in the last frame, `engine.covers()` the covers and
 * `engine.stats()` the renderer counters. Use it once `window.sloppy` exists.
 * @param {import("playwright").BrowserContext | import("playwright").Page} target
 */
export async function installEngineHelpers(target) {
  await target.addInitScript(() => {
    const game = () => window.sloppy.game;
    window.engine = {
      state: () => JSON.parse(game().debug_json()),
      view: () => JSON.parse(game().debug_view_json()),
      covers: () => JSON.parse(game().debug_covers_json()),
      stats: () => JSON.parse(game().stats_json()),
      /** Draw one still frame, from `camera = [px, py, pz, tx, ty, tz]` when given. */
      draw: (camera = [], overview = false) =>
        game().debug_render(1, 0, overview, new Float32Array(camera)),
      setTank: (id, patch) => game().debug_set_tank(id, JSON.stringify(patch)),
      setHuman: (patch) =>
        game().debug_set_tank(window.engine.state().human.id, JSON.stringify(patch)),
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
 * Choose a map in Battle Setup with pointer clicks, as a player does: a button in the row
 * of standard maps, or the dropdown that replaces it with `?debug`. Single player
 * and a new room share the "mapMode" choice.
 * @param {import("playwright").Page} page
 * @param {string} map
 * @param {string} [name]
 */
export async function chooseMap(page, map, name = "mapMode") {
  const dropdown = page.locator(`.map-picker[data-name="${name}"] .map-picker-button`);
  if (await dropdown.isVisible()) {
    await dropdown.click();
    await page.locator(`#${name}-${map}`).click();
  } else {
    await page.locator(`input[name="${name}"][value="${map}"]`).click();
  }
  await page.waitForFunction(
    ([name, map]) =>
      document.querySelector(`.map-picker[data-name="${name}"]`)?.dataset.value === map,
    [name, map],
  );
}

/** The map Battle Setup shows as chosen. @param {import("playwright").Page} page */
export function chosenMap(page, name = "mapMode") {
  return page.locator(`.map-picker[data-name="${name}"]`).getAttribute("data-value");
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
