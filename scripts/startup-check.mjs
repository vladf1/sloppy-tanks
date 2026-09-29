import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import {
  chooseMap,
  chosenMap,
  gameUrl as url,
  launchGame,
  startRound,
} from "./browser-helpers.mjs";

const output = "artifacts/performance/startup";
mkdirSync(output, { recursive: true });
const viewport = { width: 1440, height: 1000 };
const first = await launchGame({ viewport });
const { browser, errors } = first;
const results = {};
async function fresh(size = viewport) {
  const context = await browser.newContext({ viewport: size });
  const page = await context.newPage();
  page.on("pageerror", (error) => errors.push(error.message));
  return { context, page };
}
async function ready(page) {
  await page.waitForFunction(
    () => document.querySelector("#startup-overlay")?.dataset.state === "ready",
  );
}
async function playing(page) {
  await page.waitForFunction(
    () => window.sloppy?.sim.match.phase === "playing" && window.sloppy.view.time > 0,
  );
}
try {
  // The menu must remain interactive with the entire physics download held back.
  const delayed = { context: first.context, page: first.page };
  let releasePhysics;
  const physics = new Promise((resolve) => {
    releasePhysics = resolve;
  });
  await delayed.page.route(/\.wasm(?:\?|$)/, async (route) => {
    await physics;
    await route.continue();
  });
  await delayed.page.goto(url + "?map=harbor", { waitUntil: "domcontentloaded" });
  assert.equal(await delayed.page.locator("canvas").count(), 0);
  assert.equal(await chosenMap(delayed.page), "harbor");
  await delayed.page.locator('[data-kind="heavy"]').click();
  await delayed.page.locator('input[value="solo"]').check();
  await delayed.page.locator('input[value="hard"]').check();
  const startBox = await delayed.page.locator("#start").boundingBox();
  assert.ok(startBox);
  await delayed.page.mouse.click(startBox.x + startBox.width / 2, startBox.y + startBox.height / 2);
  assert.equal(await delayed.page.locator("#start").isDisabled(), true);
  assert.match(
    await delayed.page.locator("#startup-status").textContent(),
    /round|Downloading|Building|Preparing/,
  );
  // A last-minute choice during the queued start must reach the actual round.
  await chooseMap(delayed.page, "quarry");
  assert.equal(await delayed.page.locator("canvas").count(), 0);
  releasePhysics();
  await playing(delayed.page);
  results.delayed = await delayed.page.evaluate(() => ({
    tank: window.sloppy.sim.human.kind,
    map: window.sloppy.sim.mapMode,
    mode: window.sloppy.sim.gameMode,
    difficulty: window.sloppy.sim.difficulty,
    round: window.sloppy.sim.match.round,
    canvases: document.querySelectorAll("canvas#game").length,
  }));
  assert.deepEqual(results.delayed, {
    tank: "heavy",
    map: "quarry",
    mode: "solo",
    difficulty: "hard",
    round: 3,
    canvases: 1,
  });
  await delayed.context.close();

  // Hold GPU preparation too: every pipeline compiles in the background
  // (createRenderPipelineAsync), starting while the arena is built, and preparation
  // then waits for the arena's textures. GO and late choices must survive this
  // separate stage, and the simulation must not start behind the menu.
  const graphics = await fresh();
  await graphics.page.addInitScript(() => {
    const compile = GPUDevice.prototype.createRenderPipelineAsync;
    const gate = new Promise((resolve) => {
      window.releaseCompiles = resolve;
    });
    window.heldCompiles = 0;
    GPUDevice.prototype.createRenderPipelineAsync = async function (...args) {
      window.heldCompiles++;
      const pipeline = await compile.apply(this, args);
      await gate;
      return pipeline;
    };
  });
  let releaseTextures;
  const textures = new Promise((resolve) => {
    releaseTextures = resolve;
  });
  await graphics.page.route(/\/textures\/.*\.webp(?:\?|$)/, async (route) => {
    await textures;
    await route.continue();
  });
  await graphics.page.goto(url, { waitUntil: "domcontentloaded" });
  await graphics.page.waitForFunction(() => window.heldCompiles > 0);
  assert.match(
    await graphics.page.locator("#startup-status").textContent(),
    /Building the arena|Preparing graphics|Shaders loaded|Compiling shaders/,
  );
  const graphicsBox = await graphics.page.locator("#start").boundingBox();
  assert.ok(graphicsBox);
  await graphics.page.mouse.click(
    graphicsBox.x + graphicsBox.width / 2,
    graphicsBox.y + graphicsBox.height / 2,
  );
  assert.equal(await graphics.page.locator("#start").isDisabled(), true);
  assert.equal(await graphics.page.locator("#game").isVisible(), false);
  await chooseMap(graphics.page, "harbor");
  await graphics.page.screenshot({ path: `${output}/loading-queued-desktop.png` });
  assert.equal(await graphics.page.evaluate(() => window.sloppy?.sim.elapsed ?? 0), 0);
  assert.notEqual(
    await graphics.page.locator("#startup-overlay").getAttribute("data-state"),
    "ready",
  );
  await graphics.page.evaluate(() => window.releaseCompiles());
  releaseTextures();
  await playing(graphics.page);
  assert.equal(await graphics.page.evaluate(() => window.sloppy.sim.mapMode), "harbor");
  results.delayedGraphics = "early GO and changed map passed";
  await graphics.context.close();

  const warm = await fresh();
  const pickupTextures = [];
  warm.page.on("request", (request) => {
    if (request.url().includes("/textures/pickups/")) pickupTextures.push(request.url());
  });
  await warm.page.goto(url);
  await ready(warm.page);
  // Startup fetches the pickup atlas early and the engine reuses that download.
  assert.equal(pickupTextures.length, 1, "one pickup texture download");
  assert.ok(pickupTextures[0].endsWith("/textures/pickups/atlas.webp"));
  results.prepared = await warm.page.evaluate(() => {
    const { sloppy } = window;
    window.preparedGame = sloppy.game;
    // A rebuild at GO would set new options or restart the world; count both.
    window.rebuilds = 0;
    const game = Object.getPrototypeOf(sloppy.game);
    for (const name of ["set_options", "restart"]) {
      const original = game[name];
      game[name] = function (...args) {
        window.rebuilds++;
        return original.apply(this, args);
      };
    }
    return {
      time: sloppy.view.time,
      elapsed: sloppy.sim.elapsed,
      prepared: sloppy.sim.prepared,
      phase: sloppy.sim.phase,
    };
  });
  assert.deepEqual(results.prepared, { time: 0, elapsed: 0, prepared: true, phase: "ready" });
  // Each vehicle card clips the shared preview sheet: 3 kinds × 2 team rows of 640×400.
  results.previews = await warm.page.evaluate(async () => {
    const tiles = [...document.querySelectorAll(".tank-preview image")];
    const sheet = new Image();
    sheet.src = tiles[0].href.baseVal;
    await sheet.decode();
    return { cards: tiles.length, sheet: [sheet.naturalWidth, sheet.naturalHeight] };
  });
  assert.deepEqual(results.previews, { cards: 3, sheet: [1920, 800] });
  assert.equal(await warm.page.locator("#game").isVisible(), false);
  await warm.page.screenshot({ path: `${output}/desktop-menu.png` });
  await startRound(warm.page);
  await playing(warm.page);
  assert.equal(await warm.page.evaluate(() => window.rebuilds), 0, "GO reuses the arena");
  await warm.page.mouse.move(950, 450);
  await warm.page.keyboard.down("d");
  await warm.page.mouse.down();
  await warm.page.waitForFunction(
    () => window.sloppy.sim.shotsFired > 3 && window.sloppy.sim.elapsed > 0.5,
  );
  await warm.page.keyboard.up("d");
  await warm.page.mouse.up();
  await warm.page.keyboard.press("Escape");
  await warm.page.locator("#resume").waitFor();
  await warm.page.locator("#resume").click();
  await playing(warm.page);
  await warm.page.keyboard.press("Escape");
  await warm.page.locator("#end-battle").click();
  await warm.page.locator("#restart").click();
  await warm.page.locator("#start").waitFor();
  const frozen = await warm.page.evaluate(() => window.sloppy.view.time);
  await warm.page.waitForTimeout(150);
  assert.equal(await warm.page.evaluate(() => window.sloppy.view.time), frozen);
  assert.equal(await warm.page.locator("#game").isVisible(), false);
  await chooseMap(warm.page, "harbor");
  await warm.page.locator('[data-kind="scout"]').click();
  await startRound(warm.page);
  await playing(warm.page);
  results.newRound = await warm.page.evaluate(() => ({
    map: window.sloppy.sim.mapMode,
    tank: window.sloppy.sim.human.kind,
    sameEngine: window.preparedGame === window.sloppy.game,
  }));
  assert.deepEqual(results.newRound, { map: "harbor", tank: "scout", sameEngine: true });
  await warm.page.screenshot({ path: `${output}/gameplay.png` });
  await warm.context.close();

  // Changing choices after preparation rebuilds the selected arena before playing.
  const changed = await fresh();
  await changed.page.goto(url);
  await ready(changed.page);
  await changed.page.locator('input[value="solo"]').check();
  await chooseMap(changed.page, "harbor");
  await changed.page.locator('[data-kind="heavy"]').click();
  await startRound(changed.page);
  await playing(changed.page);
  assert.equal(
    await changed.page.evaluate(
      () =>
        window.sloppy.sim.gameMode === "solo" &&
        window.sloppy.sim.mapMode === "harbor" &&
        window.sloppy.sim.human.kind === "heavy",
    ),
    true,
  );
  // The chosen map is remembered and prepared behind the next visit's menu.
  await changed.page.reload();
  await ready(changed.page);
  assert.equal(await chosenMap(changed.page), "harbor");
  assert.equal(await changed.page.evaluate(() => window.sloppy.sim.mapMode), "harbor");
  await changed.context.close();

  console.log("Delayed loading, prepared arena reuse, gameplay, and new rounds passed.");
  const failure = await fresh();
  await failure.page.route(/\/src\/game\.ts(?:\?|$)/, (route) => route.abort(), { times: 1 });
  await failure.page.goto(url);
  await failure.page.waitForFunction(
    () => document.querySelector("#startup-overlay")?.dataset.state === "error",
  );
  assert.equal(await failure.page.locator("#start").isEnabled(), true);
  await failure.page.locator('input[value="easy"]').check();
  await failure.page.locator("#start").click();
  await ready(failure.page);
  assert.equal(await failure.page.locator('input[value="easy"]').isChecked(), true);
  await failure.context.close();
  results.retry = "passed";
  console.log("Failed-download retry passed.");

  // Extra levels appear, marked, only with ?extralevels, and a link can start one directly.
  // Without them the standard maps stay a row of buttons.
  const plain = await fresh();
  await plain.page.goto(url + "?map=superstress");
  await ready(plain.page);
  assert.equal(await chosenMap(plain.page), "village");
  assert.equal(await plain.page.locator('.map-picker[data-name="mapMode"]').isVisible(), false);
  assert.equal(await plain.page.locator('.map-row input[name="mapMode"]').count(), 3);
  await chooseMap(plain.page, "quarry");
  assert.equal(await plain.page.locator('input[name="mapMode"][value="quarry"]').isChecked(), true);
  await plain.page.waitForFunction(() => window.sloppy?.sim.mapName === "DUSTY DIG");
  await plain.context.close();
  for (const [level, name] of [
    ["stress-test", "STRESS GRID"],
    ["superstress", "SCRAP YARD"],
  ]) {
    const automatic = await fresh();
    await automatic.page.goto(`${url}?extralevels&map=${level}&autoplay`);
    await playing(automatic.page);
    assert.equal(await automatic.page.locator("#startup-overlay").count(), 0);
    const { tanks, map, mode, endless } = await automatic.page.evaluate(() => ({
      tanks: window.sloppy.sim.tanks.length,
      map: window.sloppy.sim.mapName,
      mode: window.sloppy.sim.gameMode,
      endless: window.sloppy.sim.endlessMatch,
    }));
    assert.deepEqual([tanks, map, mode, endless], [30, name, "team", true]);
    await automatic.context.close();
  }
  // Choosing an extra level fixes the battle format; a standard map frees it again.
  const extra = await fresh();
  await extra.page.goto(url + "?extralevels");
  await ready(extra.page);
  assert.equal(await extra.page.locator(".map-row").first().isVisible(), false);
  await extra.page.locator('input[value="solo"]').check();
  await chooseMap(extra.page, "superstress");
  assert.equal(await extra.page.locator(".map-picker-current .level-badge").textContent(), "EXTRA");
  assert.equal(await extra.page.locator('input[value="solo"]').isDisabled(), true);
  assert.equal(await extra.page.locator('input[value="team"]').isChecked(), true);
  // The level's code downloads first; the menu stays ready, as GO waits for it.
  await extra.page.waitForFunction(() => window.sloppy?.sim.mapName === "SCRAP YARD");
  await chooseMap(extra.page, "harbor");
  assert.equal(await extra.page.locator('input[value="solo"]').isDisabled(), false);
  await extra.page.waitForFunction(() => window.sloppy.sim.mapName === "HARBOR HAVOC");
  const standard = await extra.page.evaluate(() => ({
    tanks: window.sloppy.sim.tanks.length,
    endless: window.sloppy.sim.endlessMatch,
  }));
  assert.deepEqual(standard, { tanks: 12, endless: false });
  await extra.context.close();
  results.automaticStarts = "passed";
  console.log("Autoplay and extra-level startup passed.");

  for (const viewport of [{ width: 1440, height: 1000 }]) {
    const layout = await fresh(viewport);
    let releaseEngine;
    const engine = new Promise((resolve) => {
      releaseEngine = resolve;
    });
    await layout.page.route(/\/src\/game\.ts(?:\?|$)/, async (route) => {
      await engine;
      await route.continue();
    });
    await layout.page.addInitScript(() => {
      window.menuStyles = () =>
        [...document.querySelectorAll("#startup-overlay, #startup-overlay h1")].map((element) => {
          const style = getComputedStyle(element);
          const rect = element.getBoundingClientRect();
          return {
            font: style.font,
            color: style.color,
            background: style.backgroundColor,
            boxSizing: style.boxSizing,
            width: rect.width,
            height: rect.height,
            x: rect.x,
            y: rect.y,
          };
        });
    });
    await layout.page.goto(url, { waitUntil: "domcontentloaded" });
    await layout.page.locator("#startup-overlay h1").waitFor();
    await layout.page.evaluate(() => document.fonts.ready);
    const before = await layout.page.evaluate(() => window.menuStyles());
    assert.equal(before.length, 2, "authored menu is visible before the engine loads");
    await layout.page.screenshot({ path: `${output}/loading-menu-${viewport.width}.png` });
    releaseEngine();
    await ready(layout.page);
    assert.deepEqual(await layout.page.evaluate(() => window.menuStyles()), before);
    await layout.page.screenshot({ path: `${output}/menu-${viewport.width}.png` });
    assert.equal(
      await layout.page.evaluate(
        () => document.querySelector("#startup-overlay").scrollWidth <= innerWidth,
      ),
      true,
    );
    assert.equal(
      await layout.page.locator(".vehicle strong").evaluateAll((titles) =>
        titles.every((title) => {
          const range = document.createRange();
          range.selectNodeContents(title);
          const text = range.getBoundingClientRect();
          const card = title.closest(".vehicle").getBoundingClientRect();
          return text.left >= card.left + 1 && text.right <= card.right - 1;
        }),
      ),
      true,
      "vehicle titles fit within their cards without clipping",
    );
    results[`layout${viewport.width}`] = "stable inline menu; no horizontal overflow";
    await layout.context.close();
  }
  assert.deepEqual(errors, []);
  writeFileSync(`${output}/checks.json`, JSON.stringify({ ...results, errors }, null, 2));
  console.log(JSON.stringify(results));
} finally {
  await browser.close();
}
