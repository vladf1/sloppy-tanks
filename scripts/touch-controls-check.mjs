// Tablet and car-screen touch play: the phone edition's controls with the full game,
// through real CDP touch events. A drive stick in the bottom-left corner, a finger on
// the arena that aims and fires, the mine button in the bottom-right corner and the
// weapon strip between them on the bottom edge. Desktop keyboard and mouse play is
// browser-check.mjs; the phone edition is phone-check.mjs.
import {
  aimAfterTap,
  assertApart,
  assertFiring,
  assertTouchable,
  box,
  center,
  gameUrl as url,
  launchGame,
  nextFrames,
  setTouchMode,
  startRound,
  touchScreen,
} from "./browser-helpers.mjs";
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";

const output = "artifacts/performance/touch-controls";
mkdirSync(output, { recursive: true });
const { browser, page, errors } = await launchGame({
  viewport: { width: 1024, height: 768 },
  hasTouch: true,
});
try {
  await page.goto(url);
  await page.waitForFunction(() => !!window.sloppy);
  await startRound(page, { touch: true });
  // One bot, so the tank survives the whole check: a death clears every held input.
  await page.evaluate(() => {
    window.sloppy.game.debug_configure(4242, 2, 0);
    window.sloppy.start();
  });
  await page.locator(".touch-controls").waitFor({ state: "visible" });
  const { touch, tap, cancel } = await touchScreen(page);
  const state = () =>
    page.evaluate(() => {
      const { controls } = window.sloppy;
      return {
        x: controls.touch.moveX,
        fire: controls.touch.fire,
        pointers: { ...controls.touch.pointers },
      };
    });
  /** The thumb pads, weapon strip and top buttons fit on screen without overlapping,
   * the weapon strip sits on the bottom edge (never over the arena's middle), and each
   * control receives a physical touch at its centre. */
  const checkLayout = async (width, height) => {
    await page.setViewportSize({ width, height });
    await page.waitForFunction(
      ([width, height]) => innerWidth === width && innerHeight === height,
      [width, height],
    );
    await nextFrames(page);
    const selectors = [".touch-drive", ".touch-mine", ".bottom", ".hud-actions", ".scoreboard"];
    const rects = await Promise.all(selectors.map((selector) => box(page, selector)));
    assertApart(rects, selectors, width, height);
    const strip = rects[selectors.indexOf(".bottom")];
    assert.ok(
      strip.y + strip.height >= height - 40 && strip.y > height * 0.6,
      `the weapon strip stays on the bottom edge at ${width}x${height}: ${JSON.stringify(strip)}`,
    );
    await assertTouchable(
      page,
      ["#pause", "#zoom-in", "#view-mode", "#ammo-standard", ".touch-mine"],
      `at ${width}x${height}`,
    );
    await page.screenshot({ path: `${output}/${width}x${height}.png` });
  };

  await checkLayout(1024, 768);
  // A long press on the top bar, the score or the weapon strip selects no text.
  for (const selector of ["#zoom-in", "#pause", ".scoreboard", "#ammo-standard"]) {
    assert.equal(
      await page.evaluate(
        (selector) => getComputedStyle(document.querySelector(selector)).userSelect,
        selector,
      ),
      "none",
      `${selector} text is not selectable`,
    );
  }
  const drive = await center(page, ".touch-drive");
  await touch("touchStart", 1, drive.x, drive.y);
  await touch("touchMove", 1, drive.x + 35, drive.y);
  assert.ok((await state()).x > 0.4 && (await state()).x < 0.7, "analog drive");
  // A finger on the arena aims there and fires until it lifts.
  const left = await aimAfterTap(page, touch, 300, 300);
  const right = await aimAfterTap(page, touch, 820, 300);
  assert.ok(Math.sign(left) !== Math.sign(right), `taps turn the turret: ${left} → ${right}`);
  await touch("touchStart", 2, 820, 300);
  assert.ok((await state()).x > 0.4, "drives while firing");
  await assertFiring(page, "an arena finger fires");
  await tap(4, await center(page, ".touch-mine"));
  await page.waitForFunction(() => window.sloppy.sim.human.mineCooldown > 0);
  await assertFiring(page, "a mine tap does not cancel shooting");
  await page.evaluate(() => window.sloppy.giveAmmo(10));
  await tap(4, await center(page, "#ammo-rocket"));
  await assertFiring(page, "an ammo tap does not cancel shooting");
  await page.waitForFunction(() => window.sloppy.sim.human.selectedAmmo === "rocket");
  await touch("touchEnd", 2);
  await touch("touchEnd", 1);
  assert.deepEqual(await state(), {
    x: 0,
    fire: false,
    pointers: { drive: null, arena: null },
  });

  const zoom = await page.evaluate(() => window.sloppy.view.zoom);
  // Zoom reaches the engine with the next frame's input.
  await page.locator("#zoom-in").tap();
  await page.waitForFunction((zoom) => window.sloppy.view.zoom === zoom - 2, zoom);
  await page.locator("#zoom-out").tap();
  await page.waitForFunction((zoom) => window.sloppy.view.zoom === zoom, zoom);

  // First person: ◎ seats the camera in the turret without locking the pointer, a
  // sideways drag on the arena turns the view while that finger fires, and the drive
  // stick's sideways push keeps turning it.
  const firstPerson = () => page.evaluate(() => window.sloppy.view.firstPerson);
  const toggleView = await center(page, "#view-mode");
  await tap(4, toggleView);
  await page.waitForFunction(() => window.sloppy.view.firstPerson.enabled);
  assert.equal(
    await page.evaluate(() => document.pointerLockElement),
    null,
    "touch first person never locks the pointer",
  );
  // The turret view has no zoom, so − and + leave the top bar.
  await page.locator("#zoom-in").waitFor({ state: "hidden" });
  assert.equal(await page.locator("#zoom-out").isVisible(), false, "no zoom in first person");
  const yaw = (await firstPerson()).yaw;
  await touch("touchStart", 2, 420, 300);
  for (let x = 440; x <= 620; x += 20) await touch("touchMove", 2, x, 300);
  await assertFiring(page, "the turning finger fires");
  const turned = (await firstPerson()).yaw;
  assert.ok(Math.abs(turned - yaw) > 0.3, `a drag turns the view: ${yaw} → ${turned}`);
  await touch("touchEnd", 2);
  await page.screenshot({ path: `${output}/first-person.png` });
  const beforeStick = (await firstPerson()).yaw;
  await touch("touchStart", 1, drive.x, drive.y);
  await touch("touchMove", 1, drive.x + 60, drive.y);
  await page.waitForTimeout(800);
  const afterStick = (await firstPerson()).yaw;
  assert.ok(
    Math.abs(afterStick - beforeStick) > 0.8,
    `holding the stick sideways keeps turning: ${beforeStick} → ${afterStick}`,
  );
  await touch("touchEnd", 1);
  // A mouse on the same touch screen (a touch laptop) still looks with the pointer
  // lock, and the next finger frees it again.
  await page.mouse.click(520, 300);
  await page.waitForFunction(
    () => document.pointerLockElement === document.querySelector("#game"),
    null,
    { timeout: 3000 },
  );
  await tap(2, { x: 520, y: 300 });
  await page.waitForFunction(() => document.pointerLockElement === null, null, { timeout: 3000 });
  await tap(4, toggleView);
  await page.waitForFunction(() => !window.sloppy.view.firstPerson.enabled);
  await page.locator("#zoom-in").waitFor({ state: "visible" });
  assert.equal(await page.locator("#zoom-out").isVisible(), true, "zoom is back overhead");

  await touch("touchStart", 1, drive.x, drive.y);
  await touch("touchMove", 1, drive.x + 55, drive.y);
  await tap(3, await center(page, "#pause"));
  await page.locator("#resume").waitFor({ state: "visible" });
  assert.equal((await state()).x, 0, "pause clears captured movement");
  await touch("touchEnd", 1);
  assert.equal(
    await page.locator("#settings-open").isVisible(),
    false,
    "Settings never open over the pause menu",
  );
  await page.locator("#resume").tap();
  // Settings pause the battle and carry it on once closed; Cancel keeps the preference.
  await page.locator("#settings-open").tap();
  await page.locator("#touch-mode").selectOption("off");
  await page.locator(".settings-cancel").tap();
  assert.equal(await page.locator(".touch-controls").isVisible(), true, "Cancel changes nothing");
  await setTouchMode(page, "off", { touch: true });
  await page.locator(".touch-controls").waitFor({ state: "hidden" });
  await setTouchMode(page, "on", { touch: true });
  await page.locator(".touch-controls").waitFor({ state: "visible" });

  // Rotating clears a held stick; the OS cancelling every touch releases the arena finger.
  await touch("touchStart", 1, drive.x, drive.y);
  await touch("touchMove", 1, drive.x + 55, drive.y);
  await page.setViewportSize({ width: 768, height: 1024 });
  await page.waitForFunction(() => window.sloppy.controls.touch.moveX === 0);
  await touch("touchEnd", 1);
  await touch("touchStart", 5, 400, 400);
  await assertFiring(page, "an arena finger fires");
  await cancel();
  await page.waitForFunction(() => !window.sloppy.controls.touch.fire);
  assert.equal((await state()).pointers.arena, null, "OS touch cancellation releases the arena");

  // Tablets in both orientations and a car screen, where the weapon strip used to be
  // lifted over the middle of the arena.
  await checkLayout(768, 1024);
  await checkLayout(800, 640);
  await checkLayout(1280, 800);
  assert.deepEqual(errors, []);
  console.log(
    "Touch controls: drive stick, arena aim and fire, mine/ammo taps while firing, zoom, first person, pause, preference, rotation, and layout and hit-testing at 1024x768, 768x1024, 800x640 and 1280x800 passed.",
  );
} finally {
  await browser.close();
}
