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
