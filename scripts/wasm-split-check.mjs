// Hold the actual second Wasm response across several rendered menu interactions.
// Releasing it must start the chosen round exactly once, with no first-use trap.
import assert from "node:assert/strict";
import { webkit } from "playwright";
import {
  clickWidget,
  collectErrors,
  gameUrl,
  headless,
  launchChrome,
  nextFrame,
  openShell,
  untilState,
} from "./browser-helpers.mjs";

const isGameCode = (url) =>
  /-game(?:-[\w-]+)?\.wasm$/.test(url.pathname) &&
  !url.searchParams.has("import") &&
  !url.searchParams.has("url");

async function heldRound(browser, query) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const errors = [];
  collectErrors(page, errors, { consoleErrors: true });
  const downloads = [];
  page.on("request", (request) => downloads.push(new URL(request.url())));
  const held = Promise.withResolvers();
  await page.route(isGameCode, async (route) => {
    await held.promise;
    await route.continue();
  });
  try {
    await openShell(page, query);
    await clickWidget(page, "Settings");
    await untilState(page, (state) => state.settings);
    await clickWidget(page, "Close");
    await clickWidget(page, "Harbor Havoc");
    await clickWidget(page, "SKIPPER");
    await clickWidget(page, "Hard");
    await page.setViewportSize({ width: 1240, height: 810 });
    await clickWidget(page, "GO!");
    let state = await nextFrame(page);
    assert.equal(state.inRound, false);
    assert.equal(state.ready, false);
    assert.equal(state.prefs.map, "harbor");
    assert.equal(downloads.filter(isGameCode).length, 1);
    assert.ok(
      !downloads.some(
        (url) =>
          url.pathname.includes("village-ground") || url.pathname.includes("/textures/harbor/"),
      ),
      "arena downloaded behind held menu",
    );
    held.resolve();
    state = await untilState(page, (s) => s.inRound && s.phase === "playing", {
      extras: ["debug"],
      timeout: 90000,
    });
    assert.equal(state.extras.debug.mapMode, "harbor");
    assert.equal(state.extras.debug.human.kind, "scout");
    const start = state.extras.debug.human;
    await page.keyboard.down("KeyW");
    try {
      await untilState(
        page,
        (s) => Math.hypot(s.extras.debug.human.x - start.x, s.extras.debug.human.z - start.z) > 1,
        { extras: ["debug"] },
      );
    } finally {
      await page.keyboard.up("KeyW");
    }
    assert.equal(
      await page.evaluate(() => performance.getEntriesByName("sloppy:round-start").length),
      1,
    );
    assert.ok(
      !downloads.some((url) => url.pathname.includes("village-ground")),
      "fetched the discarded map",
    );
    assert.deepEqual(errors, []);
  } finally {
    held.resolve();
    await page.close();
  }
}

async function failedDownload(browser, corrupt) {
  const page = await browser.newPage();
  try {
    await page.route(
      isGameCode,
      (route) =>
        route.fulfill(
          corrupt
            ? { status: 200, contentType: "application/wasm", body: "truncated" }
            : { status: 503, body: "unavailable" },
        ),
      { times: 1 },
    );
    await page.goto(gameUrl);
    await page.waitForFunction(() => document.documentElement.dataset.shell === "failed");
    const retry = page.locator("#boot-retry");
    assert.ok(await retry.isVisible());
    assert.match(
      await page.locator("#boot-note").getAttribute("data-detail"),
      /gameplay code did not load/,
    );
    await retry.click();
    await page.waitForFunction(
      () => performance.getEntriesByName("sloppy:arena-ready").length > 0,
      null,
      { timeout: 90000 },
    );
  } finally {
    await page.close();
  }
}

for (const engine of ["WebKit", "Chrome"]) {
  const browser = engine === "WebKit" ? await webkit.launch({ headless }) : await launchChrome();
  try {
    for (const query of ["seed=42", "seed=42&webgl"]) {
      await heldRound(browser, query);
      console.log(engine, query, "held menu, queued GO and movement passed");
    }
    await failedDownload(browser, false);
    await failedDownload(browser, true);
    console.log(engine, "unavailable and corrupt downloads recovered");
  } finally {
    await browser.close();
  }
}
