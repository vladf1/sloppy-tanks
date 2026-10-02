// The limited phone edition (src/game/phone-mode.ts) on an emulated phone: Battle Setup
// offers only the tank and map, the round is single player on Easy, and the arena shows
// only the drive stick and pause (a touch on the arena aims and fires), in landscape
// and portrait, with a farther camera, no page zoom and short pause and results dialogs.
import { gameUrl as url, launchGame, startRound } from "./browser-helpers.mjs";
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";

const output = "artifacts/performance/phone";
mkdirSync(output, { recursive: true });
const { browser, context, page, errors } = await launchGame({
  viewport: { width: 844, height: 390 },
  hasTouch: true,
  isMobile: true,
});
const visible = (selectors) =>
  page.evaluate(
    (selectors) =>
      Object.fromEntries(
        selectors.map((selector) => {
          const element = document.querySelector(selector);
          const box = element?.getBoundingClientRect();
          return [selector, !!box && box.width > 0 && box.height > 0];
        }),
      ),
    selectors,
  );
const expectVisible = async (shown, hidden) => {
  const state = await visible([...shown, ...hidden]);
  for (const selector of shown) assert.equal(state[selector], true, `${selector} is shown`);
  for (const selector of hidden) assert.equal(state[selector], false, `${selector} is hidden`);
};
try {
  await page.goto(url);
  assert.equal(
    await page.evaluate(() => document.documentElement.classList.contains("phone")),
    true,
    "a small touch screen is a phone",
  );
  await page.locator("#startup-overlay[data-state=ready]").waitFor();
  await expectVisible(
    [".start .vehicles", ".map-choice", "#start"],
    [".play-tabs", ".battle-choice", ".difficulty-setting", ".menu-footer", "#multiplayer-panel"],
  );
  await page.screenshot({ path: `${output}/setup-landscape.png` });
  await page.locator('[data-kind="heavy"]').tap();
  await page.locator('input[name="mapMode"][value="harbor"]').tap({ force: true });
  await startRound(page, { touch: true });
  const hud = await page.evaluate(() => JSON.parse(window.sloppy.game.hud_json()));
  assert.equal(hud.difficulty, "easy");
  assert.equal(hud.gameMode, "team");
  assert.match(hud.mapName, /harbor/i);
  assert.equal(await page.evaluate(() => window.sloppy.view.zoom), 40, "phones start zoomed out");
  assert.equal(
    await page.evaluate(() => getComputedStyle(document.body).touchAction),
    "manipulation",
    "no double-tap page zoom",
  );
  await page.locator(".touch-controls").waitFor({ state: "visible" });
  await expectVisible(
    [".touch-drive", "#pause", "#view-mode", ".scoreboard", "#score0", "#time", "#score1"],
    [
      ".touch-aim",
      ".touch-fire",
      ".touch-mine",
      "#label0",
      "#objective",
      ".bottom",
      ".brand",
      "#settings-open",
      "#zoom-in",
      "#nerd-stats",
    ],
  );

  const session = await context.newCDPSession(page);
  const fingers = new Map();
  const touch = async (type, id, x, y) => {
    const released = fingers.get(id);
    if (type === "touchEnd") fingers.delete(id);
    else fingers.set(id, { id, x, y, radiusX: 5, radiusY: 5, force: 1 });
    await session.send("Input.dispatchTouchEvent", {
      type,
      touchPoints: type === "touchEnd" ? [released] : [...fingers.values()],
    });
    await page.evaluate(
      () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
    );
  };
  const box = async (selector) => {
    const rect = await page.locator(selector).boundingBox();
    assert.ok(rect, selector);
    return rect;
  };
  const center = async (selector) => {
    const rect = await box(selector);
    return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  };
  /** The controls and score are on screen and apart, and the controls take touches. */
  const checkLayout = async (width, height) => {
    const selectors = [".touch-drive", "#view-mode", "#pause", ".scoreboard"];
    const rects = await Promise.all(selectors.map(box));
    rects.forEach((rect, i) => {
      assert.ok(rect.x >= 0 && rect.y >= 0, `${selectors[i]} on screen`);
      assert.ok(rect.x + rect.width <= width && rect.y + rect.height <= height, selectors[i]);
      rects.slice(i + 1).forEach((other, j) => {
        const apart =
          rect.x + rect.width <= other.x ||
          other.x + other.width <= rect.x ||
          rect.y + rect.height <= other.y ||
          other.y + other.height <= rect.y;
        assert.ok(apart, `${selectors[i]} and ${selectors[i + j + 1]} do not overlap`);
      });
    });
    // The scoreboard only shows; the controls take touches.
    for (const selector of [".touch-drive", "#view-mode", "#pause"]) {
      const point = await center(selector);
      assert.equal(
        await page.evaluate(
          ({ selector, point }) =>
            document.querySelector(selector).contains(document.elementFromPoint(point.x, point.y)),
          { selector, point },
        ),
        true,
        `${selector} can receive physical touches`,
      );
    }
  };
  const state = () =>
    page.evaluate(() => ({
      x: window.sloppy.controls.touch.moveX,
      fire: window.sloppy.controls.touch.fire,
    }));

  await checkLayout(844, 390);
  const drive = await center(".touch-drive");
  await touch("touchStart", 1, drive.x, drive.y);
  await touch("touchMove", 1, drive.x + 40, drive.y);
  // A touch on the arena aims there and fires until it lifts.
  const aimAfterTap = async (x, y) => {
    await touch("touchStart", 2, x, y);
    await touch("touchMove", 2, x + 1, y);
    assert.equal((await state()).fire, true, "an arena finger fires");
    await page.waitForTimeout(400);
    await touch("touchEnd", 2);
    assert.equal((await state()).fire, false, "lifting it stops firing");
    return page.evaluate(() => window.engine.state().human.aim);
  };
  const left = await aimAfterTap(120, 170);
  const right = await aimAfterTap(724, 170);
  assert.ok(Math.sign(left) !== Math.sign(right), `taps turn the turret: ${left} → ${right}`);
  await touch("touchStart", 2, 724, 170);
  assert.ok((await state()).x > 0.4, "drives while firing");
  assert.equal((await state()).fire, true);
  await touch("touchStart", 3, 120, 170);
  assert.ok(
    await page.evaluate(() => window.sloppy.controls.nx > 0),
    "a second arena finger does not pull the aim away from the firing one",
  );
  await touch("touchEnd", 3);
  await page.screenshot({ path: `${output}/landscape.png` });
  await touch("touchEnd", 2);
  await touch("touchEnd", 1);
  assert.deepEqual(await state(), { x: 0, fire: false });

  // First person: ◎ seats the camera in the turret, a sideways drag on the arena
  // turns the view while that finger fires, and the gun sight shows.
  const firstPerson = () => page.evaluate(() => window.sloppy.view.firstPerson);
  const toggleView = await center("#view-mode");
  await touch("touchStart", 4, toggleView.x, toggleView.y);
  await touch("touchEnd", 4);
  await page.waitForFunction(() => window.sloppy.view.firstPerson.enabled);
  const yaw = (await firstPerson()).yaw;
  await touch("touchStart", 2, 420, 200);
  for (let x = 440; x <= 620; x += 20) await touch("touchMove", 2, x, 200);
  assert.equal((await state()).fire, true, "the turning finger fires");
  const turned = (await firstPerson()).yaw;
  assert.ok(Math.abs(turned - yaw) > 0.3, `a drag turns the view: ${yaw} → ${turned}`);
  await page.waitForTimeout(1000);
  assert.equal(
    await page.evaluate(() => window.engine.view().reticle.visible),
    true,
    "the first-person gun sight shows",
  );
  await expectVisible(["#cockpit", ".hull-compass"], [".aim-hint"]);
  await page.screenshot({ path: `${output}/first-person.png` });
  await touch("touchEnd", 2);
  await touch("touchStart", 4, toggleView.x, toggleView.y);
  await touch("touchEnd", 4);
  await page.waitForFunction(() => !window.sloppy.view.firstPerson.enabled);
  await page.waitForTimeout(1200);
  assert.equal(
    await page.evaluate(() => window.engine.view().reticle.visible),
    false,
    "overhead draws no reticle",
  );

  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForFunction(() => innerWidth === 390);
  await checkLayout(390, 844);
  // The narrowest phones (320 CSS px) still fit the score beside ◎ and pause.
  await page.setViewportSize({ width: 320, height: 568 });
  await page.waitForFunction(() => innerWidth === 320);
  await checkLayout(320, 568);
  await page.screenshot({ path: `${output}/narrow.png` });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForFunction(() => innerWidth === 390);
  await page.screenshot({ path: `${output}/portrait.png` });

  await page.setViewportSize({ width: 844, height: 390 });
  // What destroyed you flashes mid-screen.
  await page.evaluate(() => window.sloppy.killHuman());
  await page.locator("#toast.visible").waitFor();
  assert.notEqual((await page.locator("#toast").textContent()).trim(), "");
  await page.locator(".respawn").waitFor();
  const respawn = await page.locator(".menu.respawn").boundingBox();
  assert.ok(
    respawn.height < 390 / 2,
    `the respawn strip leaves the arena in view: ${respawn.height}`,
  );
  await page.screenshot({ path: `${output}/notice.png` });
  await page.locator("#pause").tap();
  await page.locator("#resume").waitFor({ state: "visible" });
  await expectVisible(["#pause-title", "#end-battle"], [".dialog-eyebrow", ".dialog-lede"]);
  await page.screenshot({ path: `${output}/pause.png` });
  await page.locator("#end-battle").tap();
  await page.locator("#play-again").waitFor({ state: "visible" });
  await expectVisible(
    ["#results-title", ".dialog-score", "#restart"],
    [".dialog-eyebrow", ".dialog-lede", ".recap-stats", ".recap-details", ".recap-note"],
  );
  await page.screenshot({ path: `${output}/results.png` });
  assert.deepEqual(errors, []);
  console.log(
    "Phone: tank and map setup, Easy team battle, zoomed-out camera, drive stick and touch to aim and fire, first person, landscape and portrait hit-testing, mid-screen notices, short pause and results passed.",
  );
} finally {
  await browser.close();
}
