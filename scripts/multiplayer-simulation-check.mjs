// Multiplayer seats without a server: two seats of a room simulation built like the
// server's, each driven through its own PlayerControls and drawn from its own viewer;
// plus the local speed sliders staying out of rooms and literal player names in the feed.
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import {
  gameUrl,
  headless,
  installEngineHelpers,
  seedGame,
  startRound,
} from "./browser-helpers.mjs";

const out = "artifacts/performance/multiplayer/seats";
await mkdir(out, { recursive: true });
const browser = await chromium.launch({ channel: "chrome", headless });
const errors = [];
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  await installEngineHelpers(page);
  await seedGame(page, 4242);
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  await page.addInitScript(() => {
    let frame;
    const raf = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (callback) => {
      if (callback.name !== "loop") return raf(callback);
      frame = callback;
      return 1;
    };
    window.advanceFrame = () => frame(performance.now());
  });
  await page.goto(gameUrl);
  await startRound(page);
  const advance = () =>
    page.evaluate(() => {
      for (let i = 0; i < 4; i++) window.advanceFrame();
    });
  await page.evaluate(() => {
    const { sloppy, engine } = window;
    const { human, tanks } = engine.state();
    const victim = tanks.find((tank) => !tank.human && tank.team !== human.team);
    engine.setTank(victim.id, { name: "<img src=x onerror=alert(1)>", protection: 0, shield: 0 });
    sloppy.game.debug_damage_tank(victim.id, 10000, human.id, human.team);
    for (let i = 0; i < 4; i++) window.advanceFrame();
  });
  assert.equal(await page.locator("#feed img").count(), 0);
  assert.match(await page.locator("#feed").innerText(), /YOU.*<img src=x onerror=alert\(1\)>/);
  // Battle speeds live in Settings, which pause the battle while open.
  await page.locator("#settings-open").click();
  await advance();
  assert.equal(await page.evaluate(() => window.sloppy.hud().match.phase), "paused");
  await page.locator("#tank-speed").focus();
  await page.keyboard.press("Home");
  for (let i = 0; i < 20; i++) await page.keyboard.press("ArrowRight");
  await page.locator("#bullet-speed").focus();
  await page.keyboard.press("Home");
  const unsaved = await page.evaluate(() => window.sloppy.hud().speedTuning);
  assert.deepEqual(unsaved, { "tank-speed": 1, "bullet-speed": 1 }, "nothing changes until Save");
  await page.locator(".settings-save").click();
  await advance();
  const tuning = await page.evaluate(() => window.sloppy.hud().speedTuning);
  assert.deepEqual(tuning, { "tank-speed": 1.5, "bullet-speed": 0.5 });
  assert.equal(await page.evaluate(() => window.sloppy.hud().match.phase), "playing");
  await page.locator("#settings-open").click();
  await advance();
  assert.equal(await page.locator("#tank-speed").inputValue(), "1.5");
  assert.equal(await page.locator("#bullet-speed").inputValue(), "0.5");
  await page.locator("#tank-speed").focus();
  await page.keyboard.press("End");
  await page.keyboard.press("Escape");
  await advance();
  assert.equal(await page.locator("#settings").isVisible(), false, "Esc closes Settings");
  assert.equal(
    await page.evaluate(() => window.sloppy.hud().speedTuning["tank-speed"]),
    1.5,
    "Esc cancels",
  );
  assert.equal(
    await page.evaluate(() => window.sloppy.hud().match.phase),
    "playing",
    "Esc in Settings closes them without opening the pause menu",
  );

  // The room keeps default speeds whatever this page's sliders say.
  const result = await page.evaluate(() => JSON.parse(window.sloppy.game.debug_seats(4242)));
  await page.screenshot({ path: `${out}/viewers.png` });
  assert.equal(result.viewers.length, 2);
  for (const [i, view] of result.viewers.entries()) {
    assert.equal(view.unchanged, true, "drawing a viewer leaves the room untouched");
    assert.deepEqual(view.speedTuning, { "tank-speed": 1, "bullet-speed": 1 });
    const own = view.poses.find((tank) => tank.id === view.viewerId);
    assert.ok(
      Math.hypot(own.body[0] - view.follow[0], own.body[1] - view.follow[1]) < 1e-4,
      `viewer ${i} follows its own seat: ${JSON.stringify({ own, follow: view.follow })}`,
    );
    for (const tank of view.poses) {
      assert.ok(
        Math.hypot(tank.body[0] - tank.model[0], tank.body[1] - tank.model[1]) < 1e-4,
        `seat ${tank.id} model at its body: ${JSON.stringify(tank)}`,
      );
      const expected = tank.id === view.viewerId ? 2.85 : 2.15;
      assert.ok(Math.abs(tank.barHeight - expected) < 1e-5, `bar height ${tank.barHeight}`);
    }
    assert.ok(Math.abs(result.ends[i][0] - result.starts[i][0]) > 1, `seat ${i} drove`);
  }
  assert.equal(await page.evaluate(() => window.sloppy.error()), null, "no GPU error");
  assert.deepEqual(errors, []);
  await writeFile(`${out}/browser.json`, JSON.stringify({ result, errors }, null, 2));
  console.log(
    "Two independently controlled seats and their viewers passed; tuning isolation and literal player-name feed passed.",
  );
} finally {
  await browser.close();
}
