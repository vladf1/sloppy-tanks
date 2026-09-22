import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";

const url = process.env.SLOPPY_URL ?? "http://127.0.0.1:5173/sloppy-tanks/";
const output = "artifacts/performance/touch-controls";
mkdirSync(output, { recursive: true });
const browser = await chromium.launch({ channel: "chrome", headless: false });
try {
  const context = await browser.newContext({
    viewport: { width: 1024, height: 768 },
    hasTouch: true,
    deviceScaleFactor: 1,
  });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(url);
  await page.waitForFunction(() => !!window.sloppy);
  await page.locator("#start").tap();
  await page.locator(".touch-controls").waitFor({ state: "visible" });
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
  const center = async (selector) => {
    const box = await page.locator(selector).boundingBox();
    assert.ok(box, selector);
    return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  };
  const state = () =>
    page.evaluate(() => {
      const { controls, sim } = window.sloppy;
      return {
        x: controls.touch.moveX,
        z: controls.touch.moveZ,
        fire: controls.touch.fire,
        aiming: controls.touch.aiming,
        aimX: controls.touch.aimX,
        aimY: controls.touch.aimY,
        pointers: { ...controls.touch.pointers },
        phase: sim.match.phase,
        mineCooldown: sim.human.mineCooldown,
        ammo: sim.human.selectedAmmo,
      };
    });
  const drive = await center(".touch-drive");
  const aim = await center(".touch-aim");
  await touch("touchStart", 1, drive.x, drive.y);
  await touch("touchMove", 1, drive.x + 35, drive.y);
  await touch("touchStart", 2, aim.x, aim.y);
  await touch("touchMove", 2, aim.x, aim.y - 25);
  let input = await state();
  assert.ok(input.x > 0.4 && input.x < 0.7, "analog drive");
  assert.equal(input.fire, false, "inner ring only aims");
  assert.equal(input.aimY, -1);
  await touch("touchMove", 2, aim.x, aim.y - 55);
  assert.equal((await state()).fire, true);
  const mine = await center(".touch-mine");
  await touch("touchStart", 3, mine.x, mine.y);
  await touch("touchEnd", 3);
  await page.waitForFunction(() => window.sloppy.sim.human.mineCooldown > 0);
  assert.equal((await state()).fire, true, "mine tap does not cancel shooting");
  await page.evaluate(() => {
    window.sloppy.sim.human.ammo.rocket = 10;
  });
  const ammo = await center("#ammo-rocket");
  await touch("touchStart", 3, ammo.x, ammo.y);
  await touch("touchEnd", 3);
  assert.equal((await state()).fire, true, "ammo tap does not cancel shooting");
  await page.waitForFunction(() => window.sloppy.sim.human.selectedAmmo === "rocket");
  await page.screenshot({ path: `${output}/landscape.png` });
  await touch("touchEnd", 1);
  input = await state();
  assert.equal(input.x, 0);
  assert.equal(input.fire, true);
  await touch("touchEnd", 2);
  input = await state();
  assert.equal(input.fire, false);
  assert.equal(input.aiming, true, "release retains aim");

  const zoom = await page.evaluate(() => window.sloppy.view.zoom);
  await page.locator("#zoom-in").tap();
  assert.equal(await page.evaluate(() => window.sloppy.view.zoom), zoom - 2);
  await page.locator("#zoom-out").tap();
  assert.equal(await page.evaluate(() => window.sloppy.view.zoom), zoom);

  await touch("touchStart", 1, drive.x, drive.y);
  await touch("touchMove", 1, drive.x + 55, drive.y);
  const pause = await center("#pause");
  await touch("touchStart", 3, pause.x, pause.y);
  await touch("touchEnd", 3);
  await page.locator("#resume").waitFor({ state: "visible" });
  assert.equal((await state()).x, 0, "pause clears captured movement");
  await touch("touchEnd", 1);
  await page.locator("#touch-mode").selectOption("off");
  await page.locator("#resume").tap();
  await page.locator(".touch-controls").waitFor({ state: "hidden" });
  await page.locator("#pause").tap();
  await page.locator("#touch-mode").selectOption("on");
  await page.locator("#resume").tap();
  await page.locator(".touch-controls").waitFor({ state: "visible" });

  await touch("touchStart", 1, drive.x, drive.y);
  await touch("touchMove", 1, drive.x + 55, drive.y);
  await page.setViewportSize({ width: 768, height: 1024 });
  await page.waitForFunction(() => window.sloppy.controls.touch.moveX === 0);
  await touch("touchEnd", 1);
  await page.screenshot({ path: `${output}/portrait.png` });
  const portraitAim = await center(".touch-aim");
  await touch("touchStart", 4, portraitAim.x, portraitAim.y);
  await touch("touchMove", 4, portraitAim.x + 55, portraitAim.y);
  assert.equal((await state()).fire, true);
  await session.send("Input.dispatchTouchEvent", { type: "touchCancel", touchPoints: [] });
  fingers.clear();
  await page.waitForFunction(() => !window.sloppy.controls.touch.fire);
  assert.equal((await state()).pointers.aim, null, "OS touch cancellation releases capture");
  // Safe targets remain on-screen and do not overlap the thumb pads.
  for (const selector of [
    "#pause",
    "#zoom-in",
    "#ammo-standard",
    ".touch-mine",
    ".touch-drive",
    ".touch-aim",
  ]) {
    const point = await center(selector);
    assert.ok(point.x > 0 && point.x < 768 && point.y > 0 && point.y < 1024, selector);
    assert.equal(
      await page.evaluate(
        ({ selector, point }) => {
          const target = document.querySelector(selector);
          return target.contains(document.elementFromPoint(point.x, point.y));
        },
        { selector, point },
      ),
      true,
      `${selector} can receive physical touches`,
    );
  }
  assert.deepEqual(errors, []);
  console.log(
    "Touch controls: multi-touch driving/aim/fire, third-finger mine/ammo, zoom, pause, preference, rotation and portrait hit-testing passed.",
  );
  await context.close();

  const desktop = await browser.newContext({
    viewport: { width: 1280, height: 800 },
    hasTouch: false,
  });
  const desktopPage = await desktop.newPage();
  await desktopPage.goto(url);
  await desktopPage.waitForFunction(() => !!window.sloppy);
  await desktopPage.locator("#start").click();
  assert.equal(await desktopPage.locator(".touch-controls").isVisible(), false);
  await desktopPage.keyboard.down("d");
  await desktopPage.mouse.move(600, 400);
  await desktopPage.mouse.down();
  assert.equal(
    await desktopPage.evaluate(() => {
      const command = window.sloppy.controls.command(0);
      return command.moveX === 1 && command.fire;
    }),
    true,
  );
  await desktopPage.keyboard.up("d");
  await desktopPage.mouse.up();
  console.log("Desktop: touch overlay hidden, keyboard movement and mouse fire preserved.");
} finally {
  await browser.close();
}
