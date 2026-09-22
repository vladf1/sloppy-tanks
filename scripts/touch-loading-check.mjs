import { chromium } from "playwright";
import assert from "node:assert/strict";

// Use a production build: verify that Vite actually separates JS and CSS downloads.
const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:4179/sloppy-tanks/";
const browser = await chromium.launch({ channel: "chrome", headless: false });
const isTouchAsset = (url) => /\/touch-controls[^/]*\.(js|css)(?:\?|$)/.test(url);
try {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    hasTouch: false,
  });
  const page = await context.newPage();
  const downloads = [];
  const errors = [];
  page.on("request", (request) => {
    if (isTouchAsset(request.url())) downloads.push(request.url());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url);
  await page.locator("#start").click({ timeout: 60000 });
  await page.locator("#game").waitFor({ state: "visible" });
  const initial = await page.evaluate(async () => {
    let touchMutations = 0;
    const observer = new MutationObserver((records) => {
      for (const record of records) {
        const element =
          record.target instanceof Element ? record.target : record.target.parentElement;
        if (element?.closest(".touch-controls, .touch-zoom")) touchMutations++;
      }
    });
    observer.observe(document.querySelector("#app"), {
      subtree: true,
      attributes: true,
      childList: true,
    });
    await new Promise((resolve) => {
      let count = 0;
      const frame = () => {
        if (++count === 120) resolve();
        else requestAnimationFrame(frame);
      };
      requestAnimationFrame(frame);
    });
    observer.disconnect();
    return {
      frames: 120,
      touchNodes: document.querySelectorAll(".touch-controls, .touch-controls *, .touch-zoom")
        .length,
      touchMutations,
    };
  });
  assert.equal(initial.touchNodes, 0);
  assert.equal(initial.touchMutations, 0);
  assert.equal(downloads.length, 0, "desktop never requests touch UI JS or CSS");
  console.log(
    "Desktop initial:",
    JSON.stringify(initial),
    "touch asset requests:",
    downloads.length,
  );
  await page.locator("#pause").click();
  await page.locator("#touch-mode").selectOption("on");
  await page.waitForFunction(() => document.querySelector(".touch-controls"));
  assert.ok(downloads.some((url) => url.endsWith(".js")));
  assert.ok(downloads.some((url) => url.endsWith(".css")));
  await page.locator("#resume").click();
  await page.locator(".touch-controls").waitFor({ state: "visible" });
  const assetsAfterEnable = downloads.length;
  for (let i = 0; i < 3; i++) {
    await page.locator("#pause").click();
    await page.locator("#touch-mode").selectOption("off");
    await page.locator("#resume").click();
    assert.equal(await page.locator(".touch-controls").isVisible(), false);
    const mutations = await page.evaluate(async () => {
      let count = 0;
      const observer = new MutationObserver((records) => (count += records.length));
      observer.observe(document.querySelector(".touch-controls"), {
        subtree: true,
        attributes: true,
        childList: true,
      });
      await new Promise((resolve) => {
        let frames = 0;
        const frame = () => {
          if (++frames === 12) resolve();
          else requestAnimationFrame(frame);
        };
        requestAnimationFrame(frame);
      });
      observer.disconnect();
      return count;
    });
    assert.equal(mutations, 0, "Off stops updates even after touch UI was created");
    await page.locator("#pause").click();
    assert.equal(await page.locator("#touch-mode").inputValue(), "off");
    await page.locator("#touch-mode").selectOption("on");
    await page.locator("#resume").click();
    await page.locator(".touch-controls").waitFor({ state: "visible" });
    assert.equal(await page.locator(".touch-controls").count(), 1);
  }
  assert.equal(downloads.length, assetsAfterEnable, "mode toggles reuse the loaded UI");
  assert.deepEqual(errors, []);
  console.log(
    "On/Off: deferred JS and CSS, no disabled UI mutations, no duplicate overlays or downloads.",
  );
  await context.close();

  // Turning Off while a deferred download is pending must not construct an overlay.
  const delayed = await browser.newContext({ hasTouch: false });
  const delayedPage = await delayed.newPage();
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  let requested;
  const pending = new Promise((resolve) => {
    requested = resolve;
  });
  await delayedPage.route(/\/touch-controls[^/]*\.js$/, async (route) => {
    requested();
    await gate;
    await route.continue();
  });
  await delayedPage.goto(url);
  await delayedPage.locator("#start").click({ timeout: 60000 });
  await delayedPage.locator("#game").waitFor({ state: "visible" });
  await delayedPage.locator("#pause").click();
  await delayedPage.locator("#touch-mode").selectOption("on");
  await pending;
  await delayedPage.locator("#touch-mode").selectOption("off");
  const finished = delayedPage.waitForEvent("requestfinished", (request) =>
    /\/touch-controls[^/]*\.js$/.test(request.url()),
  );
  release();
  await finished;
  await delayedPage.evaluate(
    () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
  );
  assert.equal(await delayedPage.locator(".touch-controls").count(), 0);
  await delayedPage.locator("#touch-mode").selectOption("on");
  await delayedPage.locator("#resume").click();
  await delayedPage.locator(".touch-controls").waitFor({ state: "visible" });
  console.log("Deferred download: Off prevents construction; subsequent On still works.");
  await delayed.close();

  // A real touch arriving after startup must activate Auto on a hybrid device.
  const hybrid = await browser.newContext({ hasTouch: true });
  const hybridPage = await hybrid.newPage();
  await hybridPage.addInitScript(() => {
    Object.defineProperty(navigator, "maxTouchPoints", { value: 0 });
    const original = window.matchMedia;
    window.matchMedia = (query) =>
      query === "(pointer: coarse)" ? original("not all") : original(query);
  });
  await hybridPage.goto(url);
  await hybridPage.locator("#start").click({ timeout: 60000 });
  await hybridPage.locator("#game").waitFor({ state: "visible" });
  assert.equal(await hybridPage.locator(".touch-controls").count(), 0);
  await hybridPage.touchscreen.tap(400, 300);
  await hybridPage.locator(".touch-controls").waitFor({ state: "visible" });
  console.log("Auto: first real touch activates the deferred UI on a hybrid device.");
  await hybrid.close();
} finally {
  await browser.close();
}
